"""Minimal socket probes; run using the same exec route as cargo test."""
import os
from pathlib import Path
import socket
import tempfile
import sys
failed = False

with tempfile.TemporaryDirectory(prefix="termcraft-listener-") as directory:
    root = Path(directory)
    proof = root / "regular-file"
    proof.write_text("ordinary filesystem write succeeds")
    print("Regular temporary-file write: OK")
    for family, address in [(socket.AF_INET, ("127.0.0.1", 0)),
                            (socket.AF_UNIX, str(root / "owned.sock"))]:
        phase = "socket()"
        listener = None
        try:
            listener = socket.socket(family, socket.SOCK_STREAM)
            phase = "bind()"
            listener.bind(address)
            phase = "listen()"
            listener.listen(1)
            print(f"{family.name}: socket/bind/listen OK: {listener.getsockname()}")
        except OSError as error:
            failed = True
            print(f"{family.name}: {phase} failed: errno={error.errno}: {error}")
        finally:
            if listener is not None:
                listener.close()
    print("Current process policy:")
    for line in Path("/proc/self/status").read_text().splitlines():
        if line.startswith(("Uid:", "Gid:", "NoNewPrivs:", "Seccomp:", "Seccomp_filters:", "CapEff:")):
            print(line)
    try:
        print("LSM:", Path("/proc/self/attr/current").read_text().strip())
    except OSError as error:
        print(f"LSM identification unavailable: errno={error.errno}: {error}")

runner = Path("/proc/1/cmdline").read_bytes().split(b"\0")
print("Runner executable:", os.fsdecode(runner[0]))
print("Runner applies seccomp:", b"--apply-seccomp-then-exec" in runner)
for arg in runner:
    if arg.startswith(b'{') and b'"network"' in arg:
        import json
        try:
            print("Runner network policy:", json.loads(arg).get("network"))
        except ValueError:
            pass

sys.exit(1 if failed else 0)
