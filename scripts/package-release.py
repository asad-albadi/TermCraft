#!/usr/bin/env python3
"""Build a native TermCraft release archive with a file and dependency manifest."""

import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = (Path("Cargo.toml"), Path("engines/rdp/Cargo.toml"))
ASSETS = tuple(Path("assets") / name for name in (
    "Interface.ttf", "Terminal.ttf", "Interface-LICENSE", "Terminal-LICENSE",
    "termcraft-mascot.png", "termcraft-logo.png",
))

NATIVE = {"Linux": "linux", "Windows": "windows", "Darwin": "macos"}.get(platform.system())
ARCHES = {
    "linux": {"Advanced Micro Devices X86-64": "x86_64", "AArch64": "aarch64"},
    "windows": {0x8664: "x86_64", 0xAA64: "aarch64"},
    "macos": {0x01000007: "x86_64", 0x0100000C: "aarch64"},
}


def capture(*command, **kwargs):
    return subprocess.check_output(command, text=True, **kwargs)


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def regular_source(root, relative):
    for path in (*reversed(relative.parents), relative):
        if (root / path).is_symlink():
            raise ValueError(f"Release source contains a symlink: {path}")
    path = root / relative
    if not path.is_file():
        raise ValueError(f"Expected a regular release source file: {relative}")
    return path


def copy_release_assets(source, destination):
    assets = [regular_source(source, path) for path in ASSETS]
    destination.mkdir()
    for asset in assets:
        shutil.copyfile(asset, destination / asset.name)


def stage_vnc_browser(source, resources):
    """Ship unmodified embedded viewer source/notices; never download at runtime."""
    root = source / "vendor/noVNC"
    inventory = json.loads(regular_source(source, Path("vendor/noVNC/SHA256SUMS.json")).read_text())
    expected = set(inventory) | {"SHA256SUMS.json"}
    actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
    if actual != expected:
        raise ValueError("noVNC source inventory mismatch")
    verified = []
    for relative in sorted(expected):
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts or path.as_posix() != relative:
            raise ValueError("Unsafe noVNC source path")
        original = regular_source(source, Path("vendor/noVNC") / path)
        if relative in inventory and sha256(original) != inventory[relative]:
            raise ValueError(f"noVNC source checksum mismatch: {relative}")
        verified.append((relative, original))
    destination = resources / "build-source/noVNC"
    if destination.exists():
        raise FileExistsError(destination)
    for relative, original in verified:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, target)
    metadata = json.loads((root / "UPSTREAM.json").read_text())
    return {"release": metadata["release"], "source_url": metadata["source"],
            "source_archive_sha256": metadata["sha256"], "embedded_runtime": True,
            "source_and_notices": destination.relative_to(resources).as_posix()}


def source_inputs(root):
    paths = [
        *PACKAGES, *(p.with_name("Cargo.lock") for p in PACKAGES),
        Path("engines/wayland/Cargo.toml"), Path("engines/wayland/Cargo.lock"),
        Path("CREDITS.md"), Path("FEATURE_PARITY.md"), Path("LICENSE"), Path("PRIVACY.md"),
        Path("scripts/package-release.py"), Path("scripts/package-debian.py"),
        Path("scripts/package-check-startup.py"), Path("scripts/portable_archive.py"),
        Path("scripts/build-release.sh"), *ASSETS,
    ]
    for relative in ("src", "engines/rdp/src", "engines/wayland/src", "engines/wayland/examples", "packaging", "vendor", "assets/vnc-browser"):
        paths.extend(
            p.relative_to(root) for p in (root / relative).rglob("*")
            if p.is_file() and not {".git", "target", "__pycache__"}.intersection(p.relative_to(root).parts)
        )
    paths.extend(
        path for base in (Path(), Path("engines/rdp"))
        for path in (base / "build.rs",)
        if (root / path).is_file()
    )
    return {path.as_posix(): sha256(regular_source(root, path)) for path in sorted(paths)}


def source_digest(inputs):
    encoded = json.dumps(inputs, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def freeze_sources(workspace, snapshots):
    """Capture exact source inputs and build from a reusable, content-addressed copy."""
    snapshots.mkdir(parents=True, exist_ok=True)
    for _ in range(3):
        inputs = source_inputs(workspace)
        destination = snapshots / source_digest(inputs)
        if not destination.exists():
            with tempfile.TemporaryDirectory(prefix=".capture-", dir=snapshots) as temporary:
                staged = Path(temporary) / "source"
                try:
                    for name, expected in inputs.items():
                        file = staged / name
                        file.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(regular_source(workspace, Path(name)), file)
                        if sha256(file) != expected:
                            break
                    else:
                        if source_inputs(workspace) == inputs:
                            try:
                                staged.rename(destination)
                            except OSError:
                                if not destination.is_dir():
                                    raise
                except FileNotFoundError:
                    continue
        if destination.is_dir():
            if source_inputs(destination) != inputs:
                raise ValueError(f"Stored source snapshot differs from its digest: {destination.name}")
            return destination, inputs
    raise ValueError("Source files changed while creating the snapshot. Re-run packaging.")


@contextmanager
def build_lock(root):
    # Each native Cargo target directory contains one release executable path.
    # Hold the lock until both paths have been staged into the archive.
    with (root / "package-build.lock").open("a+b") as owner:
        try:
            if os.name == "nt":
                import msvcrt
                owner.seek(0)
                msvcrt.locking(owner.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            raise ValueError("This package build directory is already in use.") from error
        yield


def cargo_invocation(state):
    """Use absolute manifests without any configuration from the checkout."""
    state.mkdir(parents=True, exist_ok=True)
    cwd = Path(state.resolve().anchor)
    if any(os.path.lexists(cwd / ".cargo" / name) for name in ("config", "config.toml")):
        raise ValueError("Cargo isolation requires a volume root without Cargo configuration.")
    cache = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo").resolve()
    variables = {
        key: value for key, value in os.environ.items()
        if not key.upper().startswith(("CARGO_", "RUSTC", "RUSTDOC", "RUSTFLAGS"))
    }
    home = state / "package-cargo-home"
    home.mkdir(mode=0o700, exist_ok=True)
    if home.is_symlink() or any(os.path.lexists(home / name) for name in ("config", "config.toml")):
        raise ValueError("The isolated package Cargo home must contain no Cargo configuration.")
    for name in ("registry", "git"):
        if (cache / name).is_dir() and not os.path.lexists(home / name):
            try:
                (home / name).symlink_to(cache / name, target_is_directory=True)
            except OSError:
                if os.name != "nt":
                    raise
                import _winapi
                _winapi.CreateJunction(str(cache / name), str(home / name))
    return {"cwd": cwd, "env": {**variables, "CARGO_HOME": str(home), "CARGO_BUILD_JOBS": "2"}}


def release_build(source, build_root, cargo_settings=None):
    if cargo_settings is None:
        cargo_settings = cargo_invocation(build_root)
    offline = ["--offline"] if os.environ.get("TERMCRAFT_OFFLINE") == "1" else []
    target_dirs = (build_root / "package-app", build_root / "package-helper")
    for manifest, target_dir in reversed(tuple(zip(PACKAGES, target_dirs, strict=True))):
        subprocess.run(
            ["cargo", "build", "--release", "--locked", *offline,
             "--manifest-path", source / manifest, "--target-dir", target_dir],
            check=True, **cargo_settings,
        )
    suffix = ".exe" if NATIVE == "windows" else ""
    return (
        target_dirs[0] / "release" / f"harbor-workspace{suffix}",
        target_dirs[1] / "release" / f"termcraft-rdp-engine{suffix}",
    )


def cargo_notices(resources, arch, target, source, cargo_settings, extra_packages=()):
    if cargo_settings is None:
        with tempfile.TemporaryDirectory(prefix="TermCraft metadata ") as temporary:
            settings = cargo_invocation(Path(temporary))
            return cargo_notices(resources, arch, target, source, settings, extra_packages)
    triple = f"{arch}-" + {
        "linux": "unknown-linux-gnu", "macos": "apple-darwin", "windows": "pc-windows-msvc"
    }[target]
    output = resources / "licenses/crates"
    crates = {}
    for manifest in (*PACKAGES, *extra_packages):
        offline = ["--offline"] if os.environ.get("TERMCRAFT_OFFLINE") == "1" else []
        metadata = json.loads(capture(
            "cargo", "metadata", "--locked", *offline, "--format-version", "1",
            "--filter-platform", triple, "--manifest-path", source / manifest, **cargo_settings,
        ))
        by_id = {crate["id"]: crate for crate in metadata["packages"]}
        root = metadata["resolve"]["root"]
        edges = {node["id"]: node["deps"] for node in metadata["resolve"]["nodes"]}
        pending = [root]
        visited = set()
        while pending:
            identifier = pending.pop()
            if identifier in visited:
                continue
            visited.add(identifier)
            pending.extend(
                dep["pkg"] for dep in edges[identifier]
                if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"])
            )
            if identifier == root:
                continue
            crate = by_id[identifier]
            key = f"{crate['name']}-{crate['version']}"
            if key in crates:
                continue
            path = Path(crate["manifest_path"]).parent
            notices = {
                p for p in path.rglob("*")
                if p.is_file() and re.search(
                    r"(^|[-_])(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE|OFL|UFL)([_.-]|$)",
                    p.name.upper(),
                )
            }
            if crate["license_file"]:
                notices.add(path / crate["license_file"])
            copied = []
            for notice in sorted(notices):
                destination = output / key / notice.relative_to(path)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(notice, destination)
                copied.append(destination.relative_to(resources).as_posix())
            crates[key] = {
                "name": crate["name"], "version": crate["version"], "license": crate["license"],
                "notice_paths": copied, "source": crate["source"] or "vendored in build source",
                "repository": crate["repository"],
            }
    return [crates[key] for key in sorted(crates)]


def binary_arch(path, target):
    if target == "linux":
        header = capture("readelf", "-h", path, env={**os.environ, "LC_ALL": "C"})
        if not re.search(r"Type:\s+(?:DYN|EXEC)\b", header):
            raise ValueError(f"Expected an executable ELF file: {path}")
        machine = re.search(r"Machine:\s+(.+)", header).group(1).strip()
    else:
        with path.open("rb") as file:
            header = file.read(64)
            if target == "windows" and header.startswith(b"MZ") and len(header) == 64:
                file.seek(struct.unpack_from("<I", header, 60)[0])
                pe = file.read(6)
                if not pe.startswith(b"PE\x00\x00") or len(pe) != 6:
                    raise ValueError(f"Invalid PE executable: {path}")
                machine = struct.unpack_from("<H", pe, 4)[0]
            elif target == "macos" and len(header) >= 16 and header.startswith(b"\xcf\xfa\xed\xfe"):
                machine = struct.unpack_from("<I", header, 4)[0]
                if struct.unpack_from("<I", header, 12)[0] != 2:
                    raise ValueError(f"Expected a Mach-O executable: {path}")
            else:
                raise ValueError(f"Unexpected executable format: {path}")
    if machine not in ARCHES[target]:
        raise ValueError(f"Unsupported {target} executable architecture: {machine}")
    return ARCHES[target][machine]


def native_libraries(path, target, private_libraries=None):
    if target == "linux":
        inspected_env = {**os.environ, "LC_ALL": "C"}
        inspected_env.pop("LD_LIBRARY_PATH", None)
        inspected_env.pop("LD_PRELOAD", None)
        inspected_env.pop("LD_AUDIT", None)
        if private_libraries is not None:
            inspected_env["LD_LIBRARY_PATH"] = str(private_libraries)
        dynamic = capture("readelf", "-d", path, env=inspected_env)
        needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic)
        if "(RPATH)" in dynamic or "(RUNPATH)" in dynamic or any("/" in name for name in needed):
            raise ValueError(f"Unexpected ELF library search path in {path.name}.")
        segments = capture("readelf", "-l", path, env=inspected_env)
        interpreter = re.search(r"Requesting program interpreter: (.+?)\]", segments)
        system_loader = r"/(?:usr/)?lib(?:64|/[a-z0-9_-]+)?/ld-linux-[a-z0-9_-]+\.so\.\d+"
        if not interpreter or not re.fullmatch(system_loader, interpreter.group(1)):
            raise ValueError(f"Unexpected ELF interpreter in {path.name}.")
        linked = capture("ldd", path, env=inspected_env)
        if "not found" in linked:
            raise ValueError(f"Missing linked libraries for {path.name}:\n{linked}")
        libraries = {
            line.strip().split()[0].rsplit("/", 1)[-1]
            for line in linked.splitlines() if line.strip()
        }
        versions = capture("readelf", "--version-info", path, env=inspected_env)
        glibc = sorted(
            set(re.findall(r"Name: GLIBC_(\d+(?:\.\d+)+)", versions)),
            key=lambda item: tuple(map(int, item.split("."))),
        )
        return {
            "libraries": sorted(libraries),
            "interpreter": interpreter.group(1),
            "highest_required_glibc_symbol_version": glibc[-1] if glibc else None,
        }
    if target == "macos":
        linked = capture("otool", "-L", path)
        libraries = [line.strip().split(" (", 1)[0] for line in linked.splitlines()[1:]]
        if any(not library.startswith(("/usr/lib/", "/System/Library/")) for library in libraries):
            raise ValueError(f"Non-system Mach-O library references for {path.name}: {libraries}")
        return {"libraries": libraries}
    if target == "windows":
        reader = shutil.which("llvm-objdump") or shutil.which("objdump")
        if reader:
            imported = capture(reader, "-p", path)
            return {"libraries": sorted(set(re.findall(r"^\s*DLL Name:\s+(\S+)", imported, re.M)))}
        dumpbin = shutil.which("dumpbin")
        if dumpbin:
            imported = capture(dumpbin, "/DEPENDENTS", path)
            return {"libraries": sorted(set(re.findall(r"^\s+(\S+\.dll)\s*$", imported, re.I | re.M)))}
    raise ValueError("DLL inspection requires llvm-objdump, objdump or dumpbin.")


def build_macos_launcher(packaging, launcher, arch):
    capture("xcrun", "clang", "-arch", "arm64" if arch == "aarch64" else "x86_64", "-fobjc-arc", "-framework", "AppKit", "-framework", "Foundation", packaging / "macos/desktop-launcher.m", "-o", launcher)
    launcher.chmod(0o755)


def macos_bundle(root, version, packaging):
    contents = root / "Contents"
    contents.mkdir()
    info = plistlib.loads((packaging / "macos/Info.plist").read_bytes())
    info.update(CFBundleVersion=version, CFBundleShortVersionString=version)
    (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    return contents / "MacOS", contents / "Resources"


def add_icon(target, resources):
    try:
        from PIL import Image
    except ImportError as error:
        raise ValueError("Install Pillow in the packaging environment to create native icons.") from error
    with Image.open(resources / "assets/termcraft-logo.png") as source:
        icon = source.convert("RGBA").resize((256, 256), Image.Resampling.LANCZOS)
    data = io.BytesIO()
    icon.save(data, format="PNG")
    logo = data.getvalue()
    if target == "macos":
        (resources / "TermCraft.icns").write_bytes(
            struct.pack(">4sI4sI", b"icns", len(logo) + 16, b"ic08", len(logo) + 8) + logo
        )
    else:
        (resources / "TermCraft.ico").write_bytes(
            struct.pack("<HHHBBBBHHII", 0, 1, 1, 0, 0, 0, 0, 1, 32, len(logo), 22) + logo
        )


def archive_permissions(member):
    member.uid = member.gid = 0
    member.uname = member.gname = "root"
    member.mode = 0o755 if member.isdir() or member.mode & 0o111 else 0o644
    return member


def stage_xwayland(source, destination, archive):
    relative = Path("packaging/x11")
    package_manifest = regular_source(source, relative / "manifest.json")
    artifacts = json.loads(package_manifest.read_text())["artifacts"]
    info = next(item for item in artifacts if item["id"] == "xwayland-24.1.15-arch-x86_64")
    entry = Path("x11/linux-x86_64/Xwayland")
    if info["entry_point"] != entry.as_posix():
        raise ValueError("Unexpected packaged Xwayland executable location.")
    if archive.is_symlink() or not archive.is_file():
        raise ValueError("Expected a regular pinned Arch Xwayland archive.")
    if sha256(archive) != info["archive_sha256"]:
        raise ValueError("Pinned Arch Xwayland archive checksum mismatch.")
    stage = regular_source(source, relative / "build-linux.sh")
    subprocess.run(["bash", stage, destination, archive.resolve(strict=True)], check=True)
    executable = regular_source(destination, entry)
    if sha256(executable) != info["executable_sha256"]:
        raise ValueError("Packaged Xwayland executable checksum mismatch.")
    dependencies = native_libraries(executable, "linux")
    return info, dependencies


def stage_xephyr(source, destination, archive):
    relative = Path("packaging/x11/xdmcp")
    info = json.loads(regular_source(source, relative / "manifest.json").read_text())
    if archive.is_symlink() or not archive.is_file():
        raise ValueError("Expected a regular pinned Arch Xephyr archive.")
    if sha256(archive) != info["archive_sha256"]:
        raise ValueError("Pinned Arch Xephyr archive checksum mismatch.")
    stage = regular_source(source, relative / "prepare-linux.py")
    subprocess.run([sys.executable, "-I", "-B", stage, destination,
                    "--archive", archive.resolve(strict=True)], check=True)
    executable = regular_source(destination, Path(info["entry_point"]))
    if sha256(executable) != info["executable_sha256"]:
        raise ValueError("Packaged Xephyr executable checksum mismatch.")
    return info, native_libraries(executable, "linux")


def stage_mosh(source, destination, inputs):
    manifest_path = regular_source(source, Path("packaging/runtime-manifest.json"))
    stage = regular_source(source, Path("packaging/runtime/stage-mosh.py"))
    subprocess.run([sys.executable, "-I", "-B", stage, manifest_path, destination, *inputs],
                   check=True)
    manifest = json.loads(manifest_path.read_text())
    client = next(item for item in manifest["artifacts"] if item["tool"] == "mosh-client")
    relative = Path("runtime/linux-x86_64/mosh-client")
    executable = regular_source(destination, relative)
    if client["path"] != relative.as_posix() or sha256(executable) != client["sha256"]:
        raise ValueError("Packaged Mosh client does not match its pinned executable.")
    return manifest, relative.as_posix(), native_libraries(executable, "linux", destination / "runtime/linux-x86_64/mosh-deps/lib")


def stage_unix(source, destination, cache, target, arch):
    stage = regular_source(source, Path("packaging/unix-runtime/stage-release.py"))
    subprocess.run([sys.executable, "-I", "-B", stage, destination,
                    cache.resolve(strict=True), target, arch], check=True)
    inventory = regular_source(destination, Path("unix-packages/INVENTORY.json"))
    return json.loads(inventory.read_text())


def browser_runtime_manifest(source, target, arch):
    name = "runtime-manifest-windows.json" if target == "windows" else "runtime-manifest.json"
    manifest = regular_source(source, Path("packaging/browser") / name)
    info = json.loads(manifest.read_text())
    if ((target, arch) != (info["platform"], info["architecture"])
            or (target == "windows" and info.get("approved") is not True)):
        raise ValueError("No qualified bundled Chromium payload for this platform.")
    return manifest, info


def stage_browser(source, resources, inputs, target, arch):
    manifest, info = browser_runtime_manifest(source, target, arch)
    stage = regular_source(source, Path("packaging/browser/stage-runtime.py"))
    subprocess.run([sys.executable, "-I", "-B", stage, manifest, resources,
                    inputs.resolve(strict=True)], check=True)
    libraries = {}
    for item in info["files"]:
        if not item["executable"]:
            continue
        relative = Path(info["root"]) / item["path"]
        path = regular_source(resources, relative)
        if target == "windows":
            if binary_arch(path, target) != arch:
                raise ValueError(f"Browser image architecture differs from the package: {relative}")
            libraries[relative.as_posix()] = native_libraries(path, target)
            continue
        if ".so" not in path.name:
            libraries[relative.as_posix()] = native_libraries(path, target, resources / info["root"] / "libraries")
            continue
        inspected_env = {**os.environ, "LC_ALL": "C"}
        for name in ("LD_LIBRARY_PATH", "LD_PRELOAD", "LD_AUDIT"):
            inspected_env.pop(name, None)
        inspected_env["LD_LIBRARY_PATH"] = str(resources / info["root"] / "libraries")
        dynamic = capture("readelf", "-d", path, env=inspected_env)
        needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic)
        if "(RPATH)" in dynamic or "(RUNPATH)" in dynamic or any("/" in name for name in needed):
            raise ValueError("Unexpected Chromium library search path")
        linked = capture("ldd", path, env=inspected_env)
        if "not found" in linked:
            raise ValueError(f"Missing browser shared libraries: {linked}")
        versions = capture("readelf", "--version-info", path, env=inspected_env)
        libraries[relative.as_posix()] = {
            "direct_libraries": sorted(needed),
            "resolved_libraries": sorted({line.strip().split()[0].rsplit("/", 1)[-1]
                                           for line in linked.splitlines() if line.strip()}),
            "required_glibc_versions": sorted(set(re.findall(r"Name: GLIBC_(\d+(?:\.\d+)+)", versions))),
        }
    return info, libraries


def package_release(target, output, app_binary, helper_binary, build_inputs=None, source=ROOT,
                    cargo_settings=None, xwayland_archive=None, mosh_inputs=None, iperf_runtime=None, unix_cache=None, browser_inputs=None, xephyr_archive=None, wayland_helper_binary=None):
    packaging = source / "packaging"
    binaries = [app_binary.resolve(strict=True), helper_binary.resolve(strict=True)]
    if binaries[0].samefile(binaries[1]):
        raise ValueError("The app and RDP helper must be distinct binaries.")
    arch, helper_arch = (binary_arch(path, target) for path in binaries)
    if arch != helper_arch:
        raise ValueError("The app and RDP helper architectures differ.")
    machine = platform.machine().lower()
    host_arch = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    if (target == "linux" or build_inputs is not None) and arch != host_arch:
        raise ValueError("The release build and library inspection require native executables.")
    app_info, helper_info = (tomllib.loads((source / p).read_text())["package"] for p in PACKAGES)
    version = app_info["version"]
    if xwayland_archive is not None and (target, arch) != ("linux", "x86_64"):
        raise ValueError("The pinned Xwayland payload targets the Linux x86_64 Arch runtime ABI.")
    if xephyr_archive is not None and (target, arch) != ("linux", "x86_64"):
        raise ValueError("The pinned Xephyr payload targets the Linux x86_64 Arch runtime ABI.")
    if iperf_runtime is not None and (target, arch) != ("linux", "x86_64"):
        raise ValueError("The qualified Iperf payload targets Linux x86_64 only.")
    if mosh_inputs is not None and (target, arch) != ("linux", "x86_64"):
        raise ValueError("The pinned Mosh payload targets the Linux x86_64 Arch runtime ABI.")
    if unix_cache is not None and (target, arch) not in (("linux", "x86_64"), ("windows", "x86_64")):
        raise ValueError("Unix payload supports Linux and Windows x86_64 only.")
    if browser_inputs is not None:
        browser_runtime_manifest(source, target, arch)
    if wayland_helper_binary is not None:
        if target != "linux" or binary_arch(wayland_helper_binary, target) != arch:
            raise ValueError("The optional Wayland helper requires the same native Linux architecture as the app.")
        if any(wayland_helper_binary.samefile(binary) for binary in binaries):
            raise ValueError("The Wayland helper must be distinct from the app and RDP helper.")
    basename = f"TermCraft-{version}-{target}-{arch}"
    if xwayland_archive is not None:
        basename += "-arch-xwayland"
    if xephyr_archive is not None:
        basename += "-xdmcp"
    if mosh_inputs is not None:
        basename += "-mosh" if xwayland_archive is not None else "-arch-mosh"
    if unix_cache is not None:
        basename += "-unix"
    if browser_inputs is not None:
        basename += "-chromium"
    if wayland_helper_binary is not None:
        basename += "-wayland-sharing"
    name = f"{basename}.tar.gz" if target == "linux" else f"{basename}.zip"
    output.mkdir(parents=True, exist_ok=True)
    archive = output / name
    if os.path.lexists(archive):
        raise FileExistsError(f"Archive already exists: {archive}")
    suffix = ".exe" if target == "windows" else ""
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as temporary:
        temp = Path(temporary)
        archive_root = "TermCraft.app" if target == "macos" else basename
        # Windows stages deep dependency notices. Keep the versioned archive
        # name out of those host paths while preserving the ZIP's public layout.
        root = temp / ("pkg" if target == "windows" else archive_root)
        root.mkdir()
        bindir, resources = macos_bundle(root, version, packaging) if target == "macos" else (root, root)
        bindir.mkdir(exist_ok=True)
        resources.mkdir(exist_ok=True)
        linked = {}
        for binary, info in zip(binaries, (app_info, helper_info), strict=True):
            executable = bindir / f"{info['name']}{suffix}"
            shutil.copyfile(binary, executable)
            executable.chmod(0o755)
            linked[executable.name] = native_libraries(executable, target)
        wayland = None
        if wayland_helper_binary is not None:
            helper_package = Path("engines/wayland/Cargo.toml")
            helper_meta = tomllib.loads(regular_source(source, helper_package).read_text())["package"]
            entry = bindir / "termcraft-wayland-engine"
            shutil.copyfile(wayland_helper_binary, entry)
            entry.chmod(0o755)
            linked[entry.name] = native_libraries(entry, target)
            if "libpipewire-0.3.so.0" not in linked[entry.name]["libraries"]:
                raise ValueError("Optional Wayland helper has no native libpipewire-0.3 ABI dependency.")
            wayland = {"entry_point": entry.name, "version": helper_meta["version"],
                       "provenance": "explicit prebuilt helper input", "sha256": sha256(entry),
                       "native_abi": "libpipewire-0.3.so.0 >= 0.3.65; matching SPA plugins; build-qualified with 1.6.11",
                       "consent": "XDG ScreenCast or RemoteDesktop portal; fresh explicit user selection",
                       "source": "build-source/engines/wayland", "license": "licenses/wayland/README.md"}
            for helper_source in ("Cargo.toml", "Cargo.lock", "src/lib.rs", "src/main.rs", "src/capture.rs", "examples/e03-wayland-synthetic-private.rs"):
                relative = Path("engines/wayland") / helper_source
                destination = resources / "build-source" / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(regular_source(source, relative), destination)
            notice = resources / "licenses/wayland/README.md"
            notice.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(regular_source(source, Path("packaging/wayland/README.md")), notice)
        xwayland = None
        if xwayland_archive is not None:
            xwayland, dependencies = stage_xwayland(source, resources, xwayland_archive)
            linked[xwayland["entry_point"]] = dependencies
        xephyr = None
        if xephyr_archive is not None:
            xephyr, dependencies = stage_xephyr(source, resources, xephyr_archive)
            linked[xephyr["entry_point"]] = dependencies
            ready = xephyr["clipboard_engine"]
            ready_binary = regular_source(resources, Path(ready["entry_point"]))
            if sha256(ready_binary) != ready["executable_sha256"]:
                raise ValueError("Packaged session-ready Xephyr checksum mismatch.")
            linked[ready["entry_point"]] = native_libraries(ready_binary, "linux")
        runtime_manifest = regular_source(source, Path("packaging/runtime-manifest.json"))
        shutil.copyfile(runtime_manifest, resources / "RUNTIME-MANIFEST.json")
        mosh = None
        if mosh_inputs is not None:
            mosh, entry, dependencies = stage_mosh(source, resources, mosh_inputs)
            linked[entry] = dependencies
        if iperf_runtime is not None:
            iperf_manifest = regular_source(source, Path("packaging/iperf/runtime-manifest.json"))
            iperf_stage = regular_source(source, Path("packaging/iperf/stage-runtime.py"))
            subprocess.run([sys.executable, "-I", "-B", iperf_stage, iperf_manifest, iperf_runtime, resources], check=True)
            for name in ("iperf2", "iperf3"):
                entry = Path("runtime/linux-x86_64") / name
                linked[entry.as_posix()] = native_libraries(regular_source(resources, entry), "linux")
        browser = None
        if browser_inputs is not None:
            browser, browser_libraries = stage_browser(source, resources, browser_inputs, target, arch)
            linked.update(browser_libraries)
        unix = stage_unix(source, resources, unix_cache, target, arch) if unix_cache is not None else None
        vnc_browser = stage_vnc_browser(source, resources)
        copy_release_assets(source, resources / "assets")
        shutil.copyfile(source / "CREDITS.md", resources / "CREDITS.md")
        if target == "windows":
            for name in ("LICENSE", "PRIVACY.md"):
                shutil.copyfile(regular_source(source, Path(name)), resources / name)
        for cargo in PACKAGES:
            relative = cargo.with_name("Cargo.lock")
            destination = resources / "build-source" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, destination)
        if target == "macos":
            launcher = bindir / "termcraft-desktop-launcher"
            shutil.copyfile(packaging / "macos/desktop-integration.py", resources / "desktop-integration.py")
            build_macos_launcher(packaging, launcher, arch)
            if binary_arch(launcher, "macos") != arch:
                raise ValueError("Native desktop launcher architecture differs from the application.")
            linked[launcher.name] = native_libraries(launcher, target)
        if target == "linux":
            shutil.copyfile(packaging / "linux/desktop-integration.py", root / "desktop-integration.py")
        else:
            add_icon(target, resources)
        if target == "windows":
            shutil.copyfile(packaging / "windows/install-per-user.ps1", root / "install-per-user.ps1")
        if target != "macos":
            shutil.copyfile(packaging / target / "README.txt", root / "README.txt")
        runtime = json.loads((packaging / "runtime-requirements.json").read_text())
        runtime["platform_requirements"] = runtime["platform_requirements"][target]
        runtime["optional_native_providers"] = runtime["optional_native_providers"][target]
        runtime["linked_libraries"] = linked
        runtime["artifact_declarations"] = "RUNTIME-MANIFEST.json"
        if wayland is not None:
            runtime["managed_wayland_sharing"] = wayland
        runtime["managed_vnc_browser"] = vnc_browser
        if iperf_runtime is not None:
            runtime["managed_iperf"] = "runtime/linux-x86_64/IPERF-MANIFEST.json"
        if browser is not None:
            runtime["managed_browser"] = browser
        if unix is not None:
            runtime["portable_unix"] = unix
        if mosh is not None:
            runtime["mosh"] = mosh["mosh_inputs"]
        if xwayland is not None:
            runtime["managed_xwayland"] = xwayland
        if xephyr is not None:
            runtime["managed_xdmcp"] = xephyr
        if target != "linux":
            del runtime["linux_display_libraries"]
        notices = cargo_notices(resources, arch, target, source, cargo_settings,
                                (Path("engines/wayland/Cargo.toml"),) if wayland is not None else ())
        inputs = source_inputs(source)
        if build_inputs is not None and inputs != build_inputs:
            raise ValueError("Frozen source snapshot changed during the release build.")
        manifest = {
            "schema_version": 1, "application": "TermCraft", "version": version,
            "profile": "release" if build_inputs is not None else "unverified",
            "rdp_helper_version": helper_info["version"], "platform": target,
            "architecture": arch, "signing": "unsigned",
            "runtime": runtime,
            "build": {
                "provenance": "local locked release build" if build_inputs is not None else "prebuilt inputs",
                "snapshot_sha256": source_digest(inputs) if build_inputs is not None else None,
                "source_inputs": inputs,
                "cargo": capture("cargo", "--version", **cargo_settings).strip() if build_inputs is not None else None,
                "rustc": capture("rustc", "--version", **cargo_settings).strip() if build_inputs is not None else None,
            },
            "licenses": {"project": "LICENSE" if target == "windows" else None,
                         "credits": (resources / "CREDITS.md").relative_to(root).as_posix(),
                         "crate_notice_paths_relative_to": resources.relative_to(root).as_posix(),
                         "crates": notices},
            "files": {p.relative_to(root).as_posix(): sha256(p) for p in sorted(root.rglob("*")) if p.is_file()},
        }
        if wayland is not None:
            manifest["wayland_helper_version"] = wayland["version"]
        (resources / "PACKAGE-MANIFEST.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        temporary_archive = temp / name
        if target == "linux":
            with tarfile.open(temporary_archive, "w:gz") as tar:
                tar.add(root, arcname=root.name, filter=archive_permissions)
        else:
            with zipfile.ZipFile(temporary_archive, "w", zipfile.ZIP_DEFLATED) as zipped:
                for path in sorted(root.rglob("*")):
                    if path.is_file():
                        zipped.write(path, f"{archive_root}/{path.relative_to(root).as_posix()}")
        # Publish a complete archive without replacing another version or concurrent output.
        if os.name == "nt":
            # Windows rename is atomic and refuses an existing destination.
            temporary_archive.rename(archive)
        else:
            os.link(temporary_archive, archive)
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=ARCHES, default=NATIVE)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--build-root", type=Path, help="Place independent Cargo target directories here.")
    parser.add_argument("--app-binary", type=Path)
    parser.add_argument("--helper-binary", type=Path)
    parser.add_argument("--xwayland-archive", type=Path,
                        help="Bundle the pinned Arch x86_64 Xwayland package (Linux only).")
    parser.add_argument("--wayland-helper-binary", type=Path, help="Optionally bundle an explicitly built native Linux PipeWire helper; ordinary app startup does not require PipeWire.")
    parser.add_argument("--xephyr-archive", type=Path, help="Bundle the pinned owned Linux x86_64 Xephyr XDMCP engine (explicit session network policy still required).")
    parser.add_argument("--iperf-runtime", type=Path, help="Root of the pinned Linux x86_64 Iperf runtime payload.")
    parser.add_argument("--mosh-archive", type=Path, help="Pinned Arch Linux x86_64 Mosh binary package.")
    parser.add_argument("--mosh-source", type=Path, help="Pinned corresponding upstream Mosh source archive.")
    parser.add_argument("--mosh-recipe", type=Path, help="Pinned corresponding Arch Mosh PKGBUILD.")
    parser.add_argument("--mosh-dependencies", type=Path, help="Offline cache of pinned Perl/OpenSSH/native dependency archives, source and recipes.")
    parser.add_argument("--unix-cache", type=Path,
                        help="Bundle target-matching pinned Unix archives from an offline upstream hash cache (Linux/Windows x86_64).")
    parser.add_argument("--browser-inputs", type=Path, help="Offline inputs for the approved platform-specific Chromium archive.")
    args = parser.parse_args()
    if bool(args.app_binary) != bool(args.helper_binary):
        parser.error("Supply both prebuilt binaries or omit both to build a native release.")
    if args.app_binary and args.build_root:
        parser.error("--build-root applies only when building from source.")
    if not args.platform:
        parser.error("Choose a supported platform.")
    if args.platform != NATIVE:
        parser.error("Run the packager on the target operating system.")
    if args.xwayland_archive and args.platform != "linux":
        parser.error("--xwayland-archive is available for Linux x86_64 only.")
    if args.xephyr_archive and args.platform != "linux":
        parser.error("--xephyr-archive is available for Linux x86_64 only.")
    if args.unix_cache is not None and args.platform not in ("linux", "windows"):
        parser.error("--unix-cache supports Linux and Windows x86_64 only.")
    if args.wayland_helper_binary is not None and args.platform != "linux":
        parser.error("--wayland-helper-binary is available for native Linux only.")
    mosh_inputs = (args.mosh_archive, args.mosh_source, args.mosh_recipe, args.mosh_dependencies)
    if any(mosh_inputs):
        if not all(mosh_inputs):
            parser.error("Use --mosh-archive, --mosh-source, --mosh-recipe, and --mosh-dependencies together.")
        if args.platform != "linux":
            parser.error("The Mosh package is available for Linux x86_64 only.")
    else:
        mosh_inputs = None
    try:
        if args.app_binary:
            archive = package_release(args.platform, args.output.resolve(), args.app_binary, args.helper_binary,
                                      xwayland_archive=args.xwayland_archive, mosh_inputs=mosh_inputs, iperf_runtime=args.iperf_runtime, unix_cache=args.unix_cache, browser_inputs=args.browser_inputs, xephyr_archive=args.xephyr_archive, wayland_helper_binary=args.wayland_helper_binary)
        else:
            build_root = args.build_root.resolve() if args.build_root else ROOT / "target"
            source, inputs = freeze_sources(ROOT, build_root / "package-sources")
            with build_lock(build_root):
                settings = cargo_invocation(build_root)
                print(f"Building source snapshot {source.name}", flush=True)
                app, helper = release_build(source, build_root, settings)
                archive = package_release(args.platform, args.output.resolve(), app, helper, inputs, source, settings,
                                          xwayland_archive=args.xwayland_archive, mosh_inputs=mosh_inputs, iperf_runtime=args.iperf_runtime, unix_cache=args.unix_cache, browser_inputs=args.browser_inputs, xephyr_archive=args.xephyr_archive, wayland_helper_binary=args.wayland_helper_binary)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")
    print(f"{sha256(archive)}  {archive}")


if __name__ == "__main__":
    main()
