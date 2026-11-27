#!/usr/bin/env python3
"""Wrap a verified Linux TermCraft archive in a Debian binary package."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

# Load from this script's directory even when imported by file-location fixtures.
import importlib.util
_archive_spec = importlib.util.spec_from_file_location(
    "termcraft_portable_archive", Path(__file__).with_name("portable_archive.py"))
_archive_module = importlib.util.module_from_spec(_archive_spec)
_archive_spec.loader.exec_module(_archive_module)
extract_tar = _archive_module.extract_tar

ROOT = Path(__file__).resolve().parents[1]
ARCHITECTURES = {"x86_64": "amd64", "aarch64": "arm64"}
POLICY_DOCUMENTS = {"LICENSE": "copyright", "PRIVACY.md": "PRIVACY.md", "CREDITS.md": "CREDITS.md"}
GRAPHICS = (
    "libegl1", "libgl1", "libwayland-client0", "libwayland-cursor0", "libwayland-egl1",
    "libxkbcommon0", "libxkbcommon-x11-0", "libx11-6", "libx11-xcb1", "libxcb1",
    "libxcursor1", "libxi6", "libxrandr2",
)
LINKED = {
    "libc.so.6": "libc6", "libm.so.6": "libc6", "libdl.so.2": "libc6",
    "libpthread.so.0": "libc6", "librt.so.1": "libc6", "libgcc_s.so.1": "libgcc-s1",
    "libstdc++.so.6": "libstdc++6",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path, algorithm="sha256"):
    with path.open("rb") as file:
        return hashlib.file_digest(file, algorithm).hexdigest()


def extract_portable(archive, path):
    extract_tar(archive, path)
    roots = list(path.iterdir())
    if len(roots) != 1 or not roots[0].is_dir():
        raise ValueError("Expected one portable package directory.")
    root = roots[0]
    manifest_path = root / "PACKAGE-MANIFEST.json"
    manifest = json.loads(manifest_path.read_text())
    if manifest["platform"] != "linux" or manifest["profile"] != "release":
        raise ValueError("Use a Linux archive produced by the locked release builder.")
    hashes = {
        p.relative_to(root).as_posix(): file_digest(p)
        for p in root.rglob("*") if p.is_file() and p != manifest_path
    }
    if hashes != manifest["files"]:
        raise ValueError("Portable archive files differ from the manifest.")
    if not {"harbor-workspace", "termcraft-rdp-engine"}.issubset(hashes):
        raise ValueError("Portable archive lacks its program or RDP helper.")
    for name in POLICY_DOCUMENTS:
        if hashes.get(name) != file_digest(ROOT / name):
            raise ValueError(f"Portable archive lacks or has a modified project policy: {name}")
    licenses = manifest.get("licenses", {})
    if licenses.get("project") != "LICENSE" or licenses.get("credits") != "CREDITS.md":
        raise ValueError("Portable archive declares incorrect project license or credits paths.")
    return root, manifest


def dependencies(manifest):
    versions = set()
    libraries = set()
    for entry in manifest["runtime"]["linked_libraries"].values():
        versions.add(entry["highest_required_glibc_symbol_version"])
        for library in entry["libraries"]:
            if library.startswith(("ld-linux-", "linux-vdso")):
                continue
            if library not in LINKED:
                raise ValueError(f"Declare a Debian dependency for {library} before packaging.")
            libraries.add(LINKED[library])
    if any(not isinstance(version, str) or not re.fullmatch(r"\d+(?:\.\d+)+", version) for version in versions):
        raise ValueError("Missing or invalid glibc version in the portable manifest.")
    minimum = max(versions, key=lambda version: tuple(map(int, version.split("."))))
    libraries.discard("libc6")
    return ", ".join((f"libc6 (>= {minimum})", *sorted(libraries), *GRAPHICS, "ca-certificates"))


def package_metadata(member):
    member.uid = member.gid = 0
    member.uname = member.gname = "root"
    member.mtime = 0
    return member


def tar_gzip(source, target):
    with target.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
        with tarfile.open(mode="w|", fileobj=compressed) as bundle:
            bundle.add(source, arcname=".", filter=package_metadata)


def write_ar(target, members):
    with target.open("wb") as output:
        output.write(b"!<arch>\n")
        for path in members:
            size = path.stat().st_size
            header = f"{path.name + '/':<16}{0:<12}{0:<6}{0:<6}{'100644':<8}{size:<10}`\n"
            output.write(header.encode("ascii"))
            with path.open("rb") as member:
                shutil.copyfileobj(member, output)
            if size % 2:
                output.write(b"\n")


def package_debian(archive, output, debian_revision=None):
    if debian_revision is not None and (
        not isinstance(debian_revision, str)
        or not re.fullmatch(r"[1-9][0-9]{0,8}", debian_revision)
    ):
        raise ValueError("Debian revision must be a canonical integer from 1 to 999999999.")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".debian-", dir=output) as temporary:
        work = Path(temporary)
        root, manifest = extract_portable(archive, work / "portable")
        version = manifest["version"]
        if not re.fullmatch(r"\d[0-9a-zA-Z.+~-]*", version):
            raise ValueError("Unsupported Debian version field.")
        if debian_revision is not None:
            version = f"{version}-{debian_revision}"
        architecture = ARCHITECTURES[manifest["architecture"]]
        destination = output / f"termcraft_{version}_{architecture}.deb"
        if os.path.lexists(destination):
            raise FileExistsError(f"Package already exists: {destination}")
        system = work / "system"
        library = system / "opt/termcraft"
        library.parent.mkdir(parents=True)
        root.rename(library)
        for name in ("harbor-workspace", "termcraft-rdp-engine"):
            (library / name).chmod(0o755)
        launcher = system / "usr/bin/termcraft"
        launcher.parent.mkdir(parents=True)
        launcher.symlink_to("/opt/termcraft/harbor-workspace")
        for relative, source in (
            ("applications/org.termcraft.TermCraft.desktop", ROOT / "packaging/debian/org.termcraft.TermCraft.desktop"),
            ("pixmaps/termcraft.png", library / "assets/termcraft-logo.png"),
            *((f"doc/termcraft/{document}", library / name)
              for name, document in POLICY_DOCUMENTS.items()),
        ):
            target = system / "usr/share" / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
        document_dir = system / "usr/share/doc/termcraft"
        staged_hashes = {item.relative_to(system).as_posix(): file_digest(item)
                         for item in sorted(system.rglob("*")) if item.is_file() and not item.is_symlink()}
        for name, document in POLICY_DOCUMENTS.items():
            expected = manifest["files"][name]
            if (staged_hashes.get(f"opt/termcraft/{name}") != expected
                    or staged_hashes.get(f"usr/share/doc/termcraft/{document}") != expected):
                raise ValueError(f"Staged Debian project policy checksum mismatch: {name}")
        (document_dir / "debian-package-manifest.json").write_text(json.dumps({
            "portable_archive_sha256": file_digest(archive),
            "builder_sha256": file_digest(Path(__file__)),
            "architecture": architecture,
            "version": version,
            "desktop_entry_sha256": file_digest(system / "usr/share/applications/org.termcraft.TermCraft.desktop"),
            "launcher": {"usr/bin/termcraft": "/opt/termcraft/harbor-workspace"},
            # The control md5sums also covers this manifest itself.
            "files": staged_hashes,
        }, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        control = work / "metadata"
        control.mkdir()
        files = [item for item in sorted(system.rglob("*")) if item.is_file() and not item.is_symlink()]
        installed_size = sum((file.stat().st_size + 1023) // 1024 for file in files)
        (control / "control").write_text(
            f"Package: termcraft\nVersion: {version}\nArchitecture: {architecture}\n"
            "Section: utils\nPriority: optional\nMaintainer: TermCraft local package build\n"
            f"Installed-Size: {installed_size}\nDepends: {dependencies(manifest)}\n"
            "Description: TermCraft remote workspace and terminal\n"
            " Local terminal, remote sessions and bundled headless RDP worker.\n",
            encoding="utf-8",
        )
        (control / "md5sums").write_text("".join(
            f"{file_digest(file, 'md5')}  {file.relative_to(system).as_posix()}\n" for file in files
        ), encoding="utf-8")
        binary = work / "debian-binary"
        binary.write_bytes(b"2.0\n")
        metadata, data = work / "control.tar.gz", work / "data.tar.gz"
        tar_gzip(control, metadata)
        tar_gzip(system, data)
        staged = work / destination.name
        write_ar(staged, (binary, metadata, data))
        os.link(staged, destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--debian-revision", help="Optional packaging revision (1–999999999); portable app version remains unchanged.")
    args = parser.parse_args()
    try:
        package = package_debian(args.archive.resolve(strict=True), args.output.resolve(), args.debian_revision)
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        parser.exit(1, f"Debian packaging failed: {error}\n")
    print(f"{file_digest(package)}  {package}")


if __name__ == "__main__":
    main()
