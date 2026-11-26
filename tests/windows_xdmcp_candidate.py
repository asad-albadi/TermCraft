"""Probe a privately built VcXsrv candidate against an isolated XDMCP peer."""

import argparse
import hashlib
import json
from pathlib import Path
import os
import secrets
import socket
import struct
import subprocess
import tempfile

def field(data):
    return struct.pack(">H", len(data)) + data

def packet(kind, payload):
    return struct.pack(">HHH", 1, kind, len(payload)) + payload


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def verify_candidate(candidate, inventory):
    contract = Path(__file__).resolve().parents[1] / "packaging/x11/native/source-contract.json"
    report = json.loads(inventory.read_text(encoding="utf-8-sig"))
    if report["source_contract_sha256"] != digest(contract):
        raise ValueError("Use an inventory from this source contract.")
    actual = {
        path.relative_to(candidate).as_posix(): digest(path)
        for path in candidate.rglob("*") if path.is_file()
    }
    if "vcxsrv.exe" not in actual or actual != report["files"]:
        raise ValueError("The private candidate does not match its staging inventory.")

def receive(peer, wanted):
    data, sender = peer.recvfrom(8192)
    version, kind, length = struct.unpack(">HHH", data[:6])
    assert version == 1 and length == len(data) - 6 and kind == wanted, (version, kind, length)
    assert sender[0] == "127.0.0.1"
    return data[6:], sender

def read_x_reply(stream):
    head = stream.read(32)
    assert len(head) == 32 and head[0] == 1, "X server failed a protocol request"
    size = struct.unpack_from("<I", head, 4)[0] * 4
    assert size <= 64 * 1024
    body = stream.read(size)
    assert len(body) == size
    return head, body


def verify_keyboard_and_extensions(client, stream, setup):
    assert len(setup) >= 32 and setup[26] <= setup[27]
    client.sendall(struct.pack("<BBHBB2x", 101, 0, 2, setup[26], 1))
    header, keysyms = read_x_reply(stream)
    assert header[1] > 0 and len(keysyms) == 4 * header[1]
    for name in (b"XKEYBOARD", b"GLX", b"RANDR", b"Composite"):
        body = name.ljust((len(name) + 3) & ~3, b"\0")
        client.sendall(struct.pack("<BBHH2x", 98, 0, 2 + len(body) // 4, len(name)) + body)
        header, _ = read_x_reply(stream)
        present = header[8] == 1
        print(f"{name.decode()} extension advertised: {present}")
        if name == b"XKEYBOARD":
            assert present
    print("Native X server returned the requested core keyboard mapping")


def run(candidate, evidence, indirect):
    with tempfile.TemporaryDirectory(prefix="TermCraft XDMCP qualification ") as scratch, socket.socket(
        socket.AF_INET, socket.SOCK_DGRAM
    ) as peer:
        peer.bind(("127.0.0.1", 0))
        peer.settimeout(8)
        with socket.socket() as candidate_port:
            candidate_port.bind(("127.0.0.1", 0))
            display = candidate_port.getsockname()[1] - 6000
        log = Path(scratch) / "server.log"
        args = [candidate / "vcxsrv.exe", f":{display}", "-noreset", "-silent-dup-error",
                "-nolisten", "inet6", "-nolisten", "hyperv", "-nopn", "-iglx", "-noclipboard",
                "-logfile", log, "-from", "127.0.0.1", "-port", str(peer.getsockname()[1]),
                "-indirect" if indirect else "-query", "127.0.0.1"]
        env = {k: os.environ[k] for k in ("SystemRoot", "WINDIR") if k in os.environ}
        env.update({"PATH": str(candidate), "TEMP": scratch, "TMP": scratch, "HOME": scratch})
        child = subprocess.Popen(args, cwd=candidate, env=env)
        try:
            _, sender = receive(peer, 3 if indirect else 2)
            peer.sendto(packet(5, field(b"") + field(b"fixture.test") + field(b"fixture willing")), sender)
            if indirect:
                print("Native IndirectQuery arrived on the isolated manager; WILLING returned")
                return
            request, sender = receive(peer, 7)
            assert struct.unpack(">H", request[:2])[0] == display
            cookie = secrets.token_bytes(16)
            accepted = struct.pack(">I", 17) + field(b"") + field(b"") + field(b"MIT-MAGIC-COOKIE-1") + field(cookie)
            peer.sendto(packet(8, accepted), sender)
            managed, _ = receive(peer, 10)
            assert struct.unpack(">IH", managed[:6]) == (17, display)
            for key, expected in [(b"", 0), (cookie, 1)]:
                with socket.create_connection(("127.0.0.1", display + 6000), 3) as client:
                    name = b"MIT-MAGIC-COOKIE-1" if key else b""
                    client.sendall(struct.pack("<BBHHHHH", ord("l"), 0, 11, 0, len(name), len(key), 0)
                                   + name.ljust((len(name) + 3) & ~3, b"\0") + key)
                    with client.makefile("rb") as stream:
                        response = stream.read(8)
                        assert len(response) == 8 and response[0] == expected
                        if expected:
                            size = struct.unpack_from("<H", response, 6)[0] * 4
                            setup = stream.read(size)
                            assert len(setup) == size
                            verify_keyboard_and_extensions(client, stream, setup)
            print("Native DirectQuery, Request, Accept and Manage passed; anonymous denied and assigned cookie accepted")
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait(5)
            if log.exists():
                label = "indirect" if indirect else "direct"
                (evidence / f"native-xdmcp-{label}.log").write_bytes(log.read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("The private VcXsrv qualification peer requires Windows.")
    candidate = args.candidate.resolve(strict=True)
    verify_candidate(candidate, args.inventory)
    args.evidence.mkdir(parents=True, exist_ok=False)
    run(candidate, args.evidence, False)
    run(candidate, args.evidence, True)


if __name__ == "__main__":
    main()
