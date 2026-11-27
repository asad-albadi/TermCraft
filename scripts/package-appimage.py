#!/usr/bin/env python3
"""Build or independently inspect the Linux x86_64 core AppImage."""
import argparse
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RECIPE = ROOT / "packaging/appimage"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


DEBIAN = load("termcraft_debian", ROOT / "scripts/package-debian.py")
MATERIALS = load("termcraft_appimage_materials", RECIPE / "materials.py")
digest = MATERIALS.digest
require = MATERIALS.require
TOOL_SHA = "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"
PORTABLE = "usr/lib/termcraft"
LIBRARY = "usr/lib/termcraft-appimage"
DOC = "usr/share/doc/termcraft-appimage"
WRAPPER = "APPIMAGE-MANIFEST.json"
DESKTOP = "org.termcraft.TermCraft.desktop"
RECIPES = ("build-runtime.sh", "relink.sh", "runtime.Dockerfile", "builder-packages.txt", "runtime-inputs.tsv", "runtime-inventory.json", "runtime-cleanup.patch",
           "library-inventory.json", "ubuntu-lock.json", "materials.py", "lock-ubuntu.py", "fetch-inputs.py", "README.txt")


def read(name):
    return json.loads((RECIPE / name).read_text())


def names(version):
    require(re.fullmatch(r"\d+(?:\.\d+){2}", version), "Invalid release version")
    base = f"TermCraft-{version}-linux-x86_64"
    return base + ".AppImage", base + "-AppImage-source.tar.gz"


def verify_prefix(path, expected, length):
    require(path.is_file() and not path.is_symlink() and 64 <= length <= 16 * 1024 * 1024, "Invalid AppImage runtime")
    with path.open("rb") as stream:
        prefix = stream.read(length)
        require(len(prefix) == length and prefix[:7] == b"\x7fELF\x02\x01\x01" and prefix[8:11] == b"AI\x02"
                and prefix[18:20] == b">\x00" and hashlib.sha256(prefix).hexdigest() == expected
                and stream.read(4) == b"hsqs", "Incorrect AppImage runtime prefix or SquashFS offset")


def source_contract(inputs=None, runtime=None, notices=None):
    """Map exact sidecar paths to independent hashes and optional producer paths."""
    contract = {}

    def add(name, sha, path=None):
        require(name not in contract, "Corresponding source path collision")
        contract[name] = (sha, path)

    for sha, name, _ in (line.split() for line in (RECIPE / "runtime-inputs.tsv").read_text().splitlines()):
        add("sources/runtime/" + name, sha, inputs / name if inputs else None)
    for package, info in read("ubuntu-lock.json")["sources"].items():
        for path, sha in info["files"].items():
            name = Path(path).name
            add(f"sources/ubuntu/{package}/{name}", sha, inputs / name if inputs else None)
    for name, sha in read("runtime-inventory.json")["files"].items():
        add("runtime/" + name, sha, runtime / name if runtime else None)
    for name, sha in read("library-inventory.json")["notices"].items():
        add("notices/ubuntu/" + name, sha, notices / name if notices else None)
    for name in RECIPES:
        path = RECIPE / name
        add("recipe/" + name, digest(path), path)
    return contract


def write_source(path, contract):
    with path.open("xb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0, compresslevel=1) as z:
        with tarfile.open(fileobj=z, mode="w|", format=tarfile.PAX_FORMAT) as tar:
            for name, (expected, source) in sorted(contract.items()):
                require(source.is_file() and not source.is_symlink() and digest(source) == expected,
                        "Missing or changed corresponding source: " + name)
                member = tarfile.TarInfo(name)
                member.size = source.stat().st_size
                member.mode = 0o755 if name.endswith((".sh", "/runtime")) else 0o644
                with source.open("rb") as stream:
                    tar.addfile(member, stream)


def verify_source(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size < 512 * 1024**2, "Invalid source sidecar")
    remaining = source_contract()
    total = 0
    # The writer emits only regular USTAR entries. Check raw blocks, including
    # padding and the EOF record, so hidden PAX overrides or appended payloads
    # cannot evade inspection. The sidecar's sources are never executed.
    with gzip.open(path, "rb") as stream:
        while True:
            header = stream.read(512)
            if header == bytes(512):
                padding = 512 + (-stream.tell() - 512) % tarfile.RECORDSIZE
                require(stream.read(padding + 1) == bytes(padding), "Unexpected data after source inventory")
                break
            try:
                member = tarfile.TarInfo.frombuf(header, "utf-8", "strict")
            except tarfile.HeaderError as error:
                raise ValueError("Invalid corresponding-source header") from error
            expected = remaining.pop(member.name, None)
            total += member.size
            require(expected and member.type == tarfile.REGTYPE and member.size >= 0 and total < 768 * 1024**2,
                    "Unexpected or oversized corresponding-source entry")
            canonical = tarfile.TarInfo(member.name)
            canonical.size = member.size
            canonical.mode = 0o755 if member.name.endswith((".sh", "/runtime")) else 0o644
            require(header == canonical.tobuf(), "Unexpected corresponding-source member metadata")
            sha = hashlib.sha256()
            unread = member.size
            while unread:
                chunk = stream.read(min(unread, 1024 * 1024))
                require(chunk, "Truncated corresponding-source member")
                sha.update(chunk)
                unread -= len(chunk)
            padding = -member.size % 512
            require(sha.hexdigest() == expected[0] and stream.read(padding) == bytes(padding),
                    "Corresponding-source bytes differ: " + member.name)
    require(not remaining, "Corresponding source is incomplete")


def core(archive, work):
    work.parent.mkdir(parents=True, exist_ok=True)
    root, manifest = DEBIAN.extract_portable(archive, work)
    version = manifest["version"]
    names(version)
    require(archive.name == f"TermCraft-{version}-linux-x86_64.tar.gz" and manifest["architecture"] == "x86_64"
            and not manifest["build"].get("gui_features") and not manifest.get("symlinks")
            and set(manifest["runtime"]) == set(json.loads((ROOT / "packaging/runtime-requirements.json").read_text()))
            | {"linked_libraries", "managed_vnc_browser", "artifact_declarations"},
            "Expected the unmodified Linux x86_64 release core")
    dependencies = json.loads((ROOT / "vendor/noVNC/SHA256SUMS.json").read_text())
    packager = load("termcraft_portable_builder", ROOT / "scripts/package-release.py")
    allowed = {"LICENSE", "PRIVACY.md", "CREDITS.md", "README.txt", "RUNTIME-MANIFEST.json",
               "desktop-integration.py", "harbor-workspace", "termcraft-rdp-engine", "build-source/Cargo.lock",
               "build-source/engines/rdp/Cargo.lock", *(p.as_posix() for p in packager.ASSETS)}
    allowed.update("build-source/noVNC/" + name for name in {*dependencies, "SHA256SUMS.json"})
    for crate in manifest["licenses"]["crates"]:
        prefix = PurePosixPath("licenses/crates") / (crate["name"] + "-" + crate["version"])
        for notice in crate["notice_paths"]:
            require(PurePosixPath(notice).is_relative_to(prefix), "Unexpected crate notice location")
            allowed.add(notice)
    require(set(manifest["files"]) == allowed, "Unexpected payload outside Linux core inventory")
    return root, manifest


def metadata(root, manifest, archive, source):
    libraries = read("library-inventory.json")
    runtime = read("runtime-inventory.json")
    versions = [v["glibc"] for v in libraries["libraries"].values() if v["glibc"]]
    for binary in ("harbor-workspace", "termcraft-rdp-engine"):
        path = root / binary
        item = MATERIALS.elf(path)
        require(item["glibc"] and set(item["needed"]) <= set(libraries["libraries"]) | MATERIALS.HOST
                and path.stat().st_mode & 0o100, "Unclassified core ELF requirements")
        versions.append(item["glibc"])
    files = {"AppRun": digest(RECIPE / "AppRun"), DESKTOP: digest(RECIPE / DESKTOP),
             "termcraft.png": digest(root / "assets/termcraft-logo.png"), DOC + "/README.txt": digest(RECIPE / "README.txt")}
    files.update({f"{LIBRARY}/{name}": item["sha256"] for name, item in libraries["libraries"].items()})
    files.update({f"{DOC}/ubuntu/{name}": sha for name, sha in libraries["notices"].items()})
    files.update({f"{DOC}/{name}": sha for name, sha in runtime["files"].items() if name.startswith("notices/")})
    return {"schema": 1, "architecture": "x86_64", "version": manifest["version"],
            "core": {"name": archive.name, "sha256": digest(archive), "manifest_sha256": digest(root / "PACKAGE-MANIFEST.json")},
            "runtime": {"source_commit": runtime["commit"], "sha256": runtime["files"]["runtime"], "size": runtime["size"]},
            "appimagetool": {"version": "1.9.1", "sha256": TOOL_SHA},
            "corresponding_source": {"name": source.name, "sha256": digest(source)},
            "libraries": libraries["libraries"], "highest_required_glibc": max(versions, key=MATERIALS.version_tuple),
            "files": files, "links": {".DirIcon": "termcraft.png"}}


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def stage(archive, inputs, runtime, source, work):
    staged = work / "TermCraft.AppDir"
    root, manifest = core(archive, work / "portable")
    actual = MATERIALS.libraries(inputs, work / "materials")
    require(actual == read("library-inventory.json"), "Ubuntu library or notice inventory changed")
    write_source(source, source_contract(inputs, runtime, work / "materials/notices"))
    info = metadata(root, manifest, archive, source)
    for name in info["files"]:
        if name.startswith(LIBRARY + "/"):
            origin = work / "materials/libraries" / Path(name).name
        elif name.startswith(DOC + "/ubuntu/"):
            origin = work / "materials/notices" / Path(name).name
        elif name.startswith(DOC + "/notices/"):
            origin = runtime / "notices" / Path(name).name
        elif name == DOC + "/README.txt":
            origin = RECIPE / "README.txt"
        elif name == "termcraft.png":
            origin = root / "assets/termcraft-logo.png"
        else:
            origin = RECIPE / name
        target = staged / name
        target.parent.mkdir(exist_ok=True, parents=True)
        shutil.copyfile(origin, target)
        target.chmod(0o755 if name == "AppRun" else 0o644)
    (staged / ".DirIcon").symlink_to("termcraft.png")
    (staged / WRAPPER).write_bytes(encoded(info))
    root.rename(staged / PORTABLE)
    return staged


def verify_image(archive, image, source, work):
    """List and validate every path, type, size and mode before confined extraction."""
    runtime = read("runtime-inventory.json")
    verify_prefix(image, runtime["files"]["runtime"], runtime["size"])
    root, manifest = core(archive, work / "core")
    require((image.name, source.name) == names(manifest["version"]), "Unexpected AppImage asset names")
    verify_source(source)
    info = metadata(root, manifest, archive, source)
    regular = {f"{PORTABLE}/{name}": sha for name, sha in manifest["files"].items()}
    regular[PORTABLE + "/PACKAGE-MANIFEST.json"] = info["core"]["manifest_sha256"]
    regular.update(info["files"])
    regular[WRAPPER] = hashlib.sha256(encoded(info)).hexdigest()
    leaves = regular.keys() | info["links"].keys()
    dirs = {str(parent) for name in leaves for parent in PurePosixPath(name).parents}
    expected = dirs | leaves
    # Bound untrusted metadata before asking unsquashfs to enumerate names.
    offset = runtime["size"]
    with image.open("rb") as stream:
        stream.seek(offset)
        superblock = stream.read(96)
        require(len(superblock) == 96, "Truncated SquashFS superblock")
        inodes = struct.unpack_from("<I", superblock, 4)[0]
        used = struct.unpack_from("<Q", superblock, 40)[0]
        require(superblock[56:64] == b"\xff" * 8, "Undeclared SquashFS xattr table")
        padded = (used + 4095) // 4096 * 4096
        require(inodes == len(expected) and 96 <= used <= 1024**3
                and image.stat().st_size == offset + padded, "Unexpected SquashFS size or inode count")
        stream.seek(offset + used)
        require(stream.read() == bytes(padded - used), "Unexpected appended AppImage bytes")
    command = ["unsquashfs", "-processors", "2", "-offset", str(runtime["size"])]
    listing = subprocess.check_output([*command, "-lln", "-d", "appdir", image], text=True, timeout=60)
    seen = set()
    total = 0
    for line in listing.splitlines():
        fields = re.fullmatch(r"(\S+)\s+(\d+/\d+)\s+(\d+)\s+\d{4}-\d\d-\d\d\s+\d\d:\d\d\s+appdir(?:/(.*))?", line)
        require(fields, "Invalid SquashFS inventory line")
        perms, owner, size, name = fields.groups()
        name = name or "."
        if perms.startswith("l"):
            name, sep, target = name.partition(" -> ")
            require(sep and info["links"].get(name) == target, "Unexpected SquashFS link")
        require(name in expected and name not in seen and owner == "0/0", "Unexpected SquashFS entry: " + name)
        seen.add(name)
        require((name in dirs and perms == "drwxr-xr-x") or
                (name in info["links"] and perms == "lrwxrwxrwx") or
                (name in regular and perms in ("-rw-r--r--", "-rwxr-xr-x")), "Unsafe SquashFS permissions")
        if name in regular:
            expected_exec = (bool((root / name.removeprefix(PORTABLE + "/")).stat().st_mode & 0o100)
                             if name.startswith(PORTABLE + "/") else name == "AppRun")
            require(expected_exec == ("x" in perms), "Executable permission differs: " + name)
            total += int(size)
            require(total < 1024**3, "Uncompressed AppImage exceeds the qualification budget")
    require(seen == expected, "AppImage inventory is incomplete")
    destination = work / "checked-appdir"
    require(not destination.exists() and not destination.is_symlink(), "Inspection destination is occupied")
    subprocess.run([*command, "-strict-errors", "-no-xattrs", "-no-progress", "-d", destination, image],
                   check=True, timeout=180, stdout=subprocess.DEVNULL)
    for name, sha in regular.items():
        member = destination / name
        require(member.is_file() and not member.is_symlink() and digest(member) == sha, "AppImage bytes differ: " + name)
    return info, destination


def stage_tool(inputs, work):
    tool = work / "appimagetool"
    with (inputs / "appimagetool").open("rb") as source, tool.open("xb") as copy:
        shutil.copyfileobj(source, copy)
    require(digest(tool) == TOOL_SHA, "Incorrect appimagetool binary")
    tool.chmod(0o700)
    return tool


def package(archive, inputs, runtime, output):
    output.mkdir(parents=True, exist_ok=True)
    match = re.fullmatch(r"TermCraft-(\d+\.\d+\.\d+)-linux-x86_64\.tar\.gz", archive.name)
    require(match, "Expected the release Linux x86_64 archive")
    image_name, source_name = names(match[1])
    destinations = (output / image_name, output / source_name)
    if any(os.path.lexists(p) for p in destinations):
        raise FileExistsError("AppImage output already exists")
    with tempfile.TemporaryDirectory(dir=output, prefix=".appimage-build-") as temp:
        work = Path(temp).resolve()
        image, source = work / image_name, work / source_name
        tool = stage_tool(inputs, work)
        staged = stage(archive, inputs, runtime, source, work)
        tool_tmp = work / "tmp"
        tool_tmp.mkdir(mode=0o700)
        subprocess.run([tool, "--appimage-extract-and-run", "--no-appstream", "--runtime-file", runtime / "runtime",
                        "--comp", "zstd", "--mksquashfs-opt", "-no-xattrs", "--mksquashfs-opt", "-processors", "--mksquashfs-opt", "2", staged, image],
                       env={"HOME": str(work), "TMPDIR": str(tool_tmp), "PATH": os.defpath, "ARCH": "x86_64",
                            "SOURCE_DATE_EPOCH": "1743465600", "LC_ALL": "C"}, check=True, timeout=300)
        # appimagetool 1.9.1 fills the optional .digest_md5 section even for
        # unsigned images. Restore the reviewed runtime's exact original bytes.
        prefix = (runtime / "runtime").read_bytes()
        with image.open("r+b") as built:
            built.seek(len(prefix))
            require(built.read(4) == b"hsqs", "Builder used an unexpected runtime size")
            built.seek(0)
            built.write(prefix)
        verify_image(archive, image, source, work / "verification")
        os.link(source, destinations[1])
        try:
            os.link(image, destinations[0])
        except BaseException:
            if os.path.samestat(source.stat(), destinations[1].lstat()):
                destinations[1].unlink()
            raise
    return destinations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--inputs", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    package(args.archive.resolve(strict=True), args.inputs.resolve(strict=True),
            args.runtime.resolve(strict=True), args.output.resolve())


if __name__ == "__main__":
    main()
