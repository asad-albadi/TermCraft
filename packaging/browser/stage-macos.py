#!/usr/bin/env python3
"""Stage the pinned Apple Silicon Chromium qualification fixture, offline."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
import subprocess
import tempfile
import zipfile

MANIFEST = Path(__file__).with_name("macos-arm64-qualification.json")
PREFIX = "chrome-mac/"


def relative(path):
    if not path or any(part in ("", ".", "..") for part in path.split("/")):
        raise ValueError(f"Invalid Chromium path: {path}")
    return PurePosixPath(path)


def extract_app(source, destination, files, links):
    """Materialize only pinned leaves. There are no symlink parents at write time."""
    expected = {PREFIX + path for path in files.keys() | links.keys()}
    directories = {parent.as_posix() for name in expected for parent in relative(name).parents}
    if files.keys() & links.keys() or expected & directories:
        raise ValueError("Overlapping Chromium paths")
    for target in links.values():
        relative(target)
    seen = set()
    with zipfile.ZipFile(source) as archive:
        for entry in archive.infolist():
            original = entry.orig_filename
            name = original.removesuffix("/")
            relative(name)
            if name in seen:
                raise ValueError(f"Duplicate Chromium archive path: {name}")
            seen.add(name)
            kind = stat.S_IFMT(entry.external_attr >> 16)
            if entry.is_dir() and kind == stat.S_IFDIR and name in directories:
                continue
            if name not in expected or entry.is_dir():
                raise ValueError(f"Unexpected Chromium archive member: {name}")
            path = name.removeprefix(PREFIX)
            if path in links:
                target = links[path].encode()
                if kind != stat.S_IFLNK or entry.file_size != len(target) or archive.read(entry) != target:
                    raise ValueError(f"Unexpected Chromium link: {path}")
                continue
            if kind != stat.S_IFREG:
                raise ValueError(f"Expected regular Chromium file: {path}")
            output = destination / path
            output.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(entry) as reader, output.open("xb") as writer:
                shutil.copyfileobj(reader, writer)
            output.chmod(0o755 if files[path] else 0o644)
    if not expected <= seen:
        raise ValueError("Incomplete Chromium archive")
    for path, target in links.items():
        (destination / path).parent.mkdir(parents=True, exist_ok=True)
        (destination / path).symlink_to(target)
    for path in links:
        (destination / path).resolve(strict=True).relative_to(destination.resolve(strict=True))


def verify_inventory(root, info):
    expected = {item["path"]: item for item in info["files"]}
    links = {item["path"]: item["target"] for item in info["links"]}
    actual = {p.relative_to(root).as_posix(): p for p in root.rglob("*")
              if p.is_symlink() or not p.is_dir()}
    if actual.keys() != expected.keys() | links.keys():
        raise ValueError("Sealed Chromium inventory differs from pin")
    for path, target in links.items():
        if not actual[path].is_symlink() or os.readlink(actual[path]) != target:
            raise ValueError(f"Sealed Chromium link differs from pin: {path}")
    for path, item in expected.items():
        node = actual[path]
        meta = node.lstat()
        if (not stat.S_ISREG(meta.st_mode) or meta.st_size != item["size"]
                or stat.S_IMODE(meta.st_mode) != (0o755 if item["executable"] else 0o644)):
            raise ValueError(f"Invalid sealed Chromium file: {path}")
        with node.open("rb") as reader:
            if hashlib.file_digest(reader, "sha256").hexdigest() != item["sha256"]:
                raise ValueError(f"Sealed Chromium checksum mismatch: {path}")
            if item["executable"]:
                reader.seek(0)
                if reader.read(8) != b"\xcf\xfa\xed\xfe\x0c\0\0\x01":
                    raise ValueError(f"Not a thin arm64 Mach-O: {path}")


def stage(destination, inputs):
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        raise ValueError("Chromium qualification requires native Apple Silicon macOS")
    manifest = MANIFEST.read_bytes()
    info = json.loads(manifest)
    final = destination / info["root"]
    if os.path.lexists(final):
        raise FileExistsError(final)
    if final.parent.is_symlink():
        raise ValueError("Browser staging parent must not be a symbolic link")
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".chromium-qualification-", dir=destination) as temporary:
        root = Path(temporary) / "payload"
        root.mkdir(mode=0o700)
        # Copy and hash the same input before giving ZIP or codesign access to it.
        archive = Path(temporary) / "archive.zip"
        src = inputs / info["archive_name"]
        with os.fdopen(os.open(src, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as reader:
            if not stat.S_ISREG(os.fstat(reader.fileno()).st_mode):
                raise ValueError("Expected regular pinned Chromium archive")
            with archive.open("xb") as writer:
                shutil.copyfileobj(reader, writer)
        with archive.open("rb") as reader:
            if hashlib.file_digest(reader, "sha256").hexdigest() != info["archive_sha256"]:
                raise ValueError("Chromium archive checksum mismatch")
        files = {item["path"]: item["executable"] for item in info["files"] if item.get("archive")}
        links = {item["path"]: item["target"] for item in info["links"]}
        extract_app(archive, root, files, links)
        app = root / "Chromium.app"
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--deep", "--timestamp=none", app], check=True)
        subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", app], check=True)
        shutil.copyfile(MANIFEST.with_name("CHROMIUM-QUALIFICATION-LICENSE"), root / "LICENSE")
        (root / "LICENSE").chmod(0o644)
        verify_inventory(root, info)
        (root / "BROWSER-MANIFEST.json").write_bytes(manifest)
        final.parent.mkdir(exist_ok=True)
        # Unlike rename, renamex_np with RENAME_EXCL cannot replace an empty directory.
        import ctypes
        rename = ctypes.CDLL(None, use_errno=True).renamex_np
        rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        if rename(os.fsencode(root), os.fsencode(final), 4) != 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error), final)
    return final


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("inputs", type=Path)
    args = parser.parse_args()
    print(stage(args.destination, args.inputs))
