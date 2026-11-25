#!/usr/bin/env python3
"""Exercise an explicitly supplied private XQuartz candidate on a native desktop."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import tempfile
import time


PROTOCOL = b"MIT-MAGIC-COOKIE-1"


def setup(path, cookie=None, query_extensions=False):
    name = PROTOCOL if cookie else b""
    packet = struct.pack("<BxHHHHxx", ord("l"), 11, 0, len(name), len(cookie or b""))
    if cookie:
        packet += name + b"\0\0" + cookie
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(1)
        client.connect(str(path))
        client.sendall(packet)
        with client.makefile("rb") as response:
            status, _, major, minor, size = struct.unpack("<BBHHH", response.read(8))
            data = response.read(size * 4)
            if len(data) != size * 4 or status not in (0, 1):
                raise ValueError("Invalid X setup reply")
            if status == 1 and (major, minor) != (11, 0):
                raise ValueError("Unexpected X protocol version")
            if query_extensions and status == 1:
                # Core X11 ListExtensions on the same authenticated connection.
                client.sendall(struct.pack("<BxH", 99, 1))
                header = response.read(32)
                if len(header) != 32 or header[0] != 1:
                    raise ValueError("XQuartz did not reply to ListExtensions")
                payload_size = struct.unpack_from("<I", header, 4)[0] * 4
                if payload_size > 65536:
                    raise ValueError("XQuartz extension payload exceeds the probe limit")
                payload = response.read(payload_size)
                extensions = []
                offset = 0
                for _ in range(header[1]):
                    if offset >= len(payload) or offset + 1 + payload[offset] > len(payload):
                        raise ValueError("Truncated XQuartz extension name")
                    end = offset + 1 + payload[offset]
                    extensions.append(payload[offset + 1:end].decode("ascii"))
                    offset = end
                return sorted(extensions)
            return status == 1


def probe(root, evidence):
    root = root.resolve(strict=True)
    binary = root / "XQuartz.app/Contents/MacOS/X11.bin"
    evidence.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="tc-xq-", dir="/private/tmp") as temporary:
        home = Path(temporary)
        path = home / "display:0"
        authority = home / "Xauthority"
        cookie = os.urandom(16)
        with os.fdopen(os.open(authority, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600), "wb") as file:
            file.write(b"\xff\xff")
            for field in (b"", b"0", PROTOCOL, cookie):
                file.write(struct.pack(">H", len(field)) + field)
        env = {
            "HOME": str(home), "TMPDIR": str(home),
            "PATH": str(root / "opt/X11/bin"), "TERMCRAFT_X11_ROOT": str(root),
            "DISPLAY": str(path), "XAUTHORITY": str(authority),
        }
        if "__CF_USER_TEXT_ENCODING" in os.environ:
            env["__CF_USER_TEXT_ENCODING"] = os.environ["__CF_USER_TEXT_ENCODING"]
        with socket.socket(socket.AF_UNIX) as listener, (evidence / "server.log").open("xb") as log:
            listener.bind(str(path))
            listener.listen(8)
            env["TERMCRAFT_OWNED_X11_FD"] = str(listener.fileno())
            command = [
                binary, ":0", "-nolisten", "tcp", "-nolisten", "local", "-noreset",
                "-iglx", "-auth", authority, "-fp", root / "opt/X11/share/fonts/X11/misc",
            ]
            child = subprocess.Popen(
                command, cwd=home, env=env, pass_fds=[listener.fileno()],
                start_new_session=True, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
            )
            try:
                deadline = time.monotonic() + 12
                while child.poll() is None:
                    try:
                        anonymous = setup(path)
                        break
                    except (TimeoutError, ConnectionRefusedError, ConnectionResetError, FileNotFoundError):
                        if time.monotonic() >= deadline:
                            raise
                else:
                    raise RuntimeError(f"XQuartz exited before readiness ({child.returncode})")
                extensions = setup(path, cookie, query_extensions=True)
                if anonymous or setup(path, bytes(a ^ 0xff for a in cookie)) or not extensions:
                    raise ValueError("Private XQuartz did not enforce the owned cookie")
                if not {"RENDER", "XKEYBOARD"} <= set(extensions) or "Composite" in extensions:
                    raise ValueError("Unexpected private XQuartz extension set")
                if os.getpgid(child.pid) != child.pid:
                    raise ValueError("XQuartz did not retain its dedicated process group")
            finally:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    child.wait(5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(5)
        result = {
            "executable_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "anonymous": "rejected", "incorrect_cookie": "rejected", "owned_cookie": "accepted",
            "owned_process_group_reaped": child.poll() is not None,
            "advertised_extensions": extensions,
        }
    if home.exists():
        raise ValueError("Private XQuartz workspace was not removed")
    result["private_runtime_removed"] = True
    (evidence / "verification.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    probe(args.candidate, args.evidence)
