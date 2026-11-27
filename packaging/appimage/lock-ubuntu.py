"""Regenerate the package lock from Ubuntu's signed Noble release index."""
import argparse
import hashlib
import json
import lzma
from pathlib import Path
import re
import subprocess

BASE = "https://snapshot.ubuntu.com/ubuntu/20250401T000000Z"
SIGNER = "F6ECB3762474EDA9D21B7022871920D1991BC93C"
SONAMES = {
    "libgcc-s1": "libgcc_s.so.1", "libx11-6": "libX11.so.6",
    "libx11-xcb1": "libX11-xcb.so.1", "libxcb1": "libxcb.so.1",
    "libxcursor1": "libXcursor.so.1", "libxi6": "libXi.so.6",
    "libxrandr2": "libXrandr.so.2", "libwayland-client0": "libwayland-client.so.0",
    "libwayland-cursor0": "libwayland-cursor.so.0", "libwayland-egl1": "libwayland-egl.so.1",
    "libxkbcommon0": "libxkbcommon.so.0", "libxkbcommon-x11-0": "libxkbcommon-x11.so.0",
    "libxau6": "libXau.so.6", "libxdmcp6": "libXdmcp.so.6", "libbsd0": "libbsd.so.0",
    "libmd0": "libmd.so.0", "libxext6": "libXext.so.6", "libxrender1": "libXrender.so.1",
    "libxfixes3": "libXfixes.so.3", "libxcb-xkb1": "libxcb-xkb.so.1", "libffi8": "libffi.so.8",
}


def records(path):
    blocks = lzma.decompress(path.read_bytes()).decode().strip().split("\n\n")
    for block in blocks:
        entry = {}
        for line in block.splitlines():
            if line.startswith(" "):
                entry[key] += "\n" + line.strip()
            else:
                key, value = line.split(":", 1)
                entry[key] = value.strip()
        yield entry


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("index", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    signed = args.index / "InRelease"
    signature = subprocess.check_output(["gpgv", "--status-fd", "1", "--keyring",
                                         args.index.resolve() / "keyring.gpg", signed], text=True)
    if not re.search(rf"^\[GNUPG:\] VALIDSIG {SIGNER} ", signature, re.M):
        raise ValueError("Unexpected Ubuntu archive signing key")
    indexes = {}
    for kind in ("binary-amd64/Packages.xz", "source/Sources.xz"):
        name = "main/" + kind
        local = args.index / name.replace("/", "-")
        digest = hashlib.sha256(local.read_bytes()).hexdigest()
        if not re.search(rf"^ {digest}\s+{local.stat().st_size} {re.escape(name)}$", signed.read_text(), re.M):
            raise ValueError("Index differs from the signed Ubuntu release")
        indexes[kind] = {r["Package"]: r for r in records(local)}
    packages = {}
    sources = {}
    for package in sorted(set(SONAMES) | {"gcc-14-base", "base-files"}):
        binary = indexes["binary-amd64/Packages.xz"][package]
        src = binary.get("Source", package).split()
        source = indexes["source/Sources.xz"][src[0]]
        if source["Version"] != (src[1].strip("()") if len(src) > 1 else binary["Version"]):
            raise ValueError("Mismatched corresponding source version")
        packages[package] = {"version": binary["Version"], "source": src[0], "path": binary["Filename"],
                             "architecture": binary["Architecture"], "sha256": binary["SHA256"],
                             "soname": SONAMES.get(package)}
        files = {}
        for line in source["Checksums-Sha256"].splitlines():
            if line:
                sha, _, filename = line.split()
                files[source["Directory"] + "/" + filename] = sha
        sources[src[0]] = {"version": source["Version"], "files": files}
    args.output.write_text(json.dumps({"snapshot": BASE, "suite": "noble", "signer": SIGNER,
                                       "inrelease_sha256": hashlib.sha256(signed.read_bytes()).hexdigest(),
                                       "packages": packages, "sources": sources},
                                      indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
