#!/usr/bin/env python3
"""Check the Linux candidate against an owned Weston headless socket, without X11."""

import argparse
import importlib.util
import os
from pathlib import Path
import signal
import socket
import stat
import struct
import subprocess
import tempfile
import time

SPEC = importlib.util.spec_from_file_location("startup", Path(__file__).with_name("package-check-startup.py"))
STARTUP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STARTUP)


def wait_socket(server, path, deadline):
    while time.monotonic() < deadline and server.poll() is None:
        if path.is_socket():
            try:
                with socket.socket(socket.AF_UNIX) as probe:
                    probe.settimeout(0.2)
                    probe.connect(str(path))
                    peer_pid, peer_uid, _ = struct.unpack("3i", probe.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
                    if peer_pid != server.pid or peer_uid != os.getuid():
                        raise ValueError("The private Wayland socket belongs to an unexpected process")
                    return
            except (ConnectionRefusedError, FileNotFoundError, TimeoutError):
                pass
        time.sleep(0.05)
    raise ValueError("Private Weston socket did not become ready")


def check(archive, evidence, source, log, appimage_core=None):
    if os.getuid() == 0:
        raise ValueError("Run the private Wayland fixture as an ordinary user")
    log.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="termcraft-weston-") as scratch, log.open("xb") as diagnostics:
        runtime = Path(scratch)
        if stat.S_IMODE(runtime.stat().st_mode) != 0o700:
            raise ValueError("Weston runtime directory is not private")
        name = "termcraft-release-wayland"
        path = runtime / name
        environment = {name: value for name, value in os.environ.items()
                       if name not in {"DISPLAY", "WAYLAND_SOCKET", "WAYLAND_DISPLAY", "WAYLAND_DEBUG"}}
        environment.update(XDG_RUNTIME_DIR=str(runtime), LIBGL_ALWAYS_SOFTWARE="1")
        with subprocess.Popen(
            ["weston", "--backend=headless-backend.so", "--renderer=pixman", "--socket=" + name,
             "--width=1440", "--height=1024", "--no-config", "--idle-time=0"],
            env=environment, start_new_session=True, stdout=diagnostics, stderr=subprocess.STDOUT,
        ) as server:
            try:
                wait_socket(server, path, time.monotonic() + 20)
                inode = path.stat().st_ino
                STARTUP.smoke(archive, evidence, None, native_wayland={
                    "source_sha": source, "environment": {**environment, "WAYLAND_DISPLAY": name},
                    "socket": path, "weston_pid": server.pid, "socket_inode": inode,
                }, appimage_core=appimage_core)
                if server.poll() is not None or not path.is_socket() or path.stat().st_ino != inode:
                    raise ValueError("Owned compositor or socket changed during the startup check")
            finally:
                try:
                    os.killpg(server.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(server.pid, signal.SIGKILL)
                    server.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--source", required=True)
    parser.add_argument("--weston-log", required=True, type=Path)
    parser.add_argument("--appimage-core", type=Path)
    args = parser.parse_args()
    try:
        check(args.archive.resolve(strict=True), args.evidence.resolve(), args.source, args.weston_log, args.appimage_core)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Native Wayland check failed: {error}\n")


if __name__ == "__main__":
    main()
