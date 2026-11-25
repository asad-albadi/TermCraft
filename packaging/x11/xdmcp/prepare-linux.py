#!/usr/bin/env python3
"""Stage pinned Xephyr or an isolated xdm fixture without installing packages."""
import argparse
import hashlib
import io
import json
import lzma
import pathlib
import subprocess
import tarfile
import urllib.request

MAX_ARCHIVE = 16 * 1024 * 1024


def pinned_archive(info, local_path):
    if local_path is not None:
        with local_path.open("rb") as stream:
            data = stream.read(MAX_ARCHIVE + 1)
    else:
        with urllib.request.urlopen(info["archive_url"], timeout=30) as stream:
            data = stream.read(MAX_ARCHIVE + 1)
    if len(data) > MAX_ARCHIVE or hashlib.sha256(data).hexdigest() != info["archive_sha256"]:
        raise SystemExit("Pinned archive verification failed")
    unpacked = subprocess.run(["zstd", "-dc"], input=data,
                              stdout=subprocess.PIPE, check=True).stdout
    return tarfile.open(fileobj=io.BytesIO(unpacked))


def regular_bytes(archive, name):
    member = archive.getmember(name)
    if not member.isfile() or member.size > MAX_ARCHIVE:
        raise SystemExit("Invalid regular file in pinned package")
    return archive.extractfile(member).read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--fixture", action="store_true")
    parser.add_argument("--archive", type=pathlib.Path)
    parser.add_argument("--fixture-library-archive", type=pathlib.Path)
    args = parser.parse_args()
    manifest = json.loads(pathlib.Path(__file__).with_name("manifest.json").read_text())
    entry = manifest["fixture"] if args.fixture else manifest
    files = {
        "usr/bin/xdm": "xdm",
        "usr/lib/X11/xdm/libXdmGreet.so": "libXdmGreet.so",
    } if args.fixture else {
        "usr/bin/Xephyr": "x11/linux-x86_64/Xephyr",
        ".BUILDINFO": "x11/linux-x86_64/XEPHYR_BUILDINFO",
        ".PKGINFO": "x11/linux-x86_64/XEPHYR_PKGINFO",
        "usr/share/licenses/xorg-server-xephyr/COPYING": "x11/linux-x86_64/XEPHYR_COPYING",
    }
    with pinned_archive(entry, args.archive) as archive:
        for source, target in files.items():
            content = regular_bytes(archive, source)
            if source == "usr/bin/Xephyr" and hashlib.sha256(content).hexdigest() != manifest["executable_sha256"]:
                raise SystemExit("Pinned executable verification failed")
            path = args.output / target
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            path.chmod(0o755 if source in ("usr/bin/Xephyr", "usr/bin/xdm") else 0o644)
    if not args.fixture:
        ready = manifest["clipboard_engine"]
        asset = pathlib.Path(__file__).with_name("readiness") / "Xephyr-session-ready.xz"
        compressed = asset.read_bytes()
        if hashlib.sha256(compressed).hexdigest() != ready["compressed_sha256"]:
            raise SystemExit("Pinned session-ready compressed executable verification failed")
        data = lzma.decompress(compressed)
        if len(data) > MAX_ARCHIVE or hashlib.sha256(data).hexdigest() != ready["executable_sha256"]:
            raise SystemExit("Pinned session-ready executable verification failed")
        target = args.output / ready["entry_point"]
        target.write_bytes(data)
        target.chmod(0o755)
        notice = (asset.parent / "XTRANS_COPYING").read_bytes()
        if hashlib.sha256(notice).hexdigest() != ready["xtrans_license_sha256"]:
            raise SystemExit("Unexpected xtrans license inventory")
        (args.output / ready["xtrans_license"]).write_bytes(notice)
    if args.fixture:
        library = manifest["fixture"]["private_library"]
        with pinned_archive(library, args.fixture_library_archive) as archive:
            content = regular_bytes(archive, library["entry"])
        if hashlib.sha256(content).hexdigest() != library["entry_sha256"]:
            raise SystemExit("Pinned fixture library verification failed")
        target = args.output / "lib/libXpm.so.4"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        target.chmod(0o644)
        support = pathlib.Path(__file__).resolve().parents[3] / "tests/support"
        for name in ("xdmcp_window.py", "xdmcp_output.py"):
            (args.output / name).write_bytes((support / name).read_bytes())
    print(args.output.resolve())


if __name__ == "__main__":
    main()
