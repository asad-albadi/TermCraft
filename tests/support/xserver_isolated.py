"""Activate Cage's isolated display :0, then run the packaged-engine tests."""

import os
import pathlib
import socket
import subprocess
import sys
import time


def compositor_xwayland():
    for process in pathlib.Path("/proc").iterdir():
        if not process.name.isdecimal():
            continue
        try:
            command = (process / "cmdline").read_bytes().split(b"\0")
        except (OSError, PermissionError):
            continue
        if pathlib.Path(os.fsdecode(command[0])).name == "Xwayland" and command[1:2] == [b":0"]:
            return process
    return None


def run():
    socket_dir = pathlib.Path("/tmp/.X11-unix")
    identity = socket_dir.stat()
    assert f"{identity.st_dev}:{identity.st_ino}" != os.environ["TERMCRAFT_TEST_HOST_SOCKET_DIR"]
    assert os.readlink("/proc/self/ns/net") != os.environ["TERMCRAFT_TEST_HOST_NETWORK"]
    os.environ["TERMCRAFT_TEST_XSERVER_ISOLATED"] = "1"

    # Cage starts Xwayland on demand after this first connection to its socket.
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(8)
        client.connect(str(socket_dir / "X0"))
        client.sendall(b"l\0\x0b" + b"\0" * 9)
        reply = client.recv(8)
        if len(reply) != 8 or reply[0] != 1:
            raise RuntimeError("Cage's isolated X11 fixture :0 did not become ready")
        deadline = time.monotonic() + 3
        while (process := compositor_xwayland()) is None:
            if time.monotonic() >= deadline:
                raise RuntimeError("the pre-existing isolated Xwayland fixture was not found")
            time.sleep(0.02)
        os.environ["TERMCRAFT_TEST_XSERVER_FIXTURE_PID"] = process.name
        result = subprocess.run(sys.argv[1:], timeout=45, check=False)
        if not (process / "exe").exists():
            raise RuntimeError("the pre-existing isolated X server exited during the test")
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(run())
