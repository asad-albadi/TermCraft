#!/usr/bin/env python3
"""Collect, revalidate and deliver source-bound, unsigned desktop candidates."""

import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import math
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import plistlib
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
import zlib

from portable_archive import extract_tar, extract_zip


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "linux": ("x86_64", "linux-amd64", ".tar.gz"),
    "windows": ("x86_64", "windows-amd64", ".zip"),
    "macos": ("aarch64", "macos-arm64", ".zip"),
}
MANIFESTS = {
    "Cargo.toml": "harbor-workspace",
    "engines/rdp/Cargo.toml": "termcraft-rdp-engine",
    "engines/wayland/Cargo.toml": "termcraft-wayland-engine",
}
RUST = "1.96.1"
SCOPE = "unsigned-core"
VERSION = re.compile(r"(?:0|[1-9][0-9]{0,8})(?:\.(?:0|[1-9][0-9]{0,8})){2}")
SHA1 = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"[0-9a-f]{64}")
SAFE_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,199}")
REPO = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9][A-Za-z0-9_.-]*")
STARTUP_FILES = {"verification.json", "startup.png", "startup.log", "help.log",
                 "rdp-helper.log", "rdp-session.log"}
PENDING_GATES = ("linux-appimage-x11-wayland-source", "linux-native-wayland", "two-version-lifecycle-linux",
                 "two-version-lifecycle-macos", "two-version-lifecycle-windows")


def load_sibling(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGER = load_sibling("package-release")
DEBIAN = load_sibling("package-debian")
STARTUP = load_sibling("package-check-startup")
APPIMAGE = load_sibling("package-appimage")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def previous_version(version):
    require(VERSION.fullmatch(version), "Expected a canonical numeric candidate version")
    components = list(map(int, version.split(".")))
    for index in reversed(range(len(components))):
        if components[index]:
            components[index] -= 1
            return ".".join(map(str, components))
    raise ValueError("No supported version exists below 0.0.0")


def patched_version_bytes(name, content, old, new):
    import tomllib

    if name == "Cargo.toml":
        text = content.split(b"\n[", 2)
        require(text[0].startswith(b"[package]\n") and
                tomllib.loads(content.decode())["package"]["name"] == "harbor-workspace",
                "Expected the root application package")
        before = text[0]
        suffix = b"\n[" + b"\n[".join(text[1:])
        pattern = rb'(?m)^version = "' + re.escape(old.encode()) + rb'"$'
    else:
        require(name == "Cargo.lock", "Unexpected private version input")
        before, suffix = content, b""
        pattern = rb'(?m)(^\[\[package\]\]\nname = "harbor-workspace"\n)version = "' + re.escape(old.encode()) + rb'"$'
    patched, count = re.subn(pattern, lambda m: (m[1] if m.lastindex else b"") +
                             f'version = "{new}"'.encode(), before)
    require(count == 1, f"Expected one application version entry in {name}")
    return patched + suffix


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, f"Duplicate JSON key: {key}")
        value[key] = item
    return value


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def write_json(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(value, indent=2, sort_keys=True) + "\n")


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def package_versions(root):
    versions = {}
    for name, expected in MANIFESTS.items():
        info = tomllib.loads((root / name).read_text(encoding="utf-8"))["package"]
        value = info["version"]
        require(info["name"] == expected and isinstance(value, str) and VERSION.fullmatch(value),
                f"Invalid package name or version in {name}")
        versions[expected] = value
    return versions


def publish_event(event, ref, version):
    return bool(VERSION.fullmatch(version) and event == "push" and ref == f"refs/tags/v{version}")


def version_for(root, event, ref):
    version = package_versions(root)["harbor-workspace"]
    require(event in {"push", "workflow_dispatch"}, "Unsupported release event")
    if event == "push" or ref.startswith("refs/tags/"):
        require(ref == f"refs/tags/v{version}", "Version tag must exactly match the application version")
    return version


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def verify_checkout(root, source, version):
    require(SHA1.fullmatch(source), "Expected a lowercase 40-character source commit SHA")
    require(git(root, "rev-parse", "HEAD").decode().strip() == source, "Source commit differs from checkout")
    require(not git(root, "status", "--porcelain", "--untracked-files=no"), "Release checkout has tracked changes")
    require(package_versions(root)["harbor-workspace"] == version, "Application version differs from preflight")


def regular_entries(directory):
    require(not directory.is_symlink() and directory.is_dir(), f"Expected owned directory: {directory}")
    entries = {}
    for path in directory.iterdir():
        require(SAFE_NAME.fullmatch(path.name) and not path.is_symlink() and path.is_file(),
                f"Unexpected file entry: {path.name}")
        entries[path.name] = path
    return entries


def file_inventory(root, exclude=None):
    files, links = {}, {}
    for path in root.rglob("*"):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            links[name] = os.readlink(path)
        elif path.is_file() and path != exclude:
            files[name] = sha256(path)
    return files, links


def portable_name(platform, version):
    arch, _, extension = TARGETS[platform]
    return f"TermCraft-{version}-{platform}-{arch}{extension}"


def canonical_crates(crates):
    # Path ordering on Windows folds case, unlike PosixPath ordering.
    return [{**crate, "notice_paths": sorted(crate["notice_paths"])} for crate in crates]


def expected_dependency_notices(root, platform, work):
    """Resolve both locked graphs from trusted sources, never the candidate."""
    resources = work / "expected-notices"
    crates = PACKAGER.cargo_notices(resources, TARGETS[platform][0], platform, root, None)
    files, links = file_inventory(resources)
    require(crates and files and not links, "Could not generate the locked dependency notice inventory")
    return {"crates": canonical_crates(crates), "files": files}


def core_layout(root, package, files, links, manifest, platform, versions, expected_notices):
    """Only the product, adjacent helpers, integration, policies and core notices."""
    macos = platform == "macos"
    resources = "Contents/Resources/" if macos else ""
    binaries = "Contents/MacOS/" if macos else ""
    suffix = ".exe" if platform == "windows" else ""
    executables = ["harbor-workspace" + suffix, "termcraft-rdp-engine" + suffix]
    originals = {
        "LICENSE": "LICENSE", "PRIVACY.md": "PRIVACY.md", "CREDITS.md": "CREDITS.md",
        "RUNTIME-MANIFEST.json": "packaging/runtime-manifest.json",
        "build-source/Cargo.lock": "Cargo.lock",
        "build-source/engines/rdp/Cargo.lock": "engines/rdp/Cargo.lock",
        "README.txt": f"packaging/{platform}/README.txt",
        **{path.as_posix(): path.as_posix() for path in PACKAGER.ASSETS},
    }
    novnc_root = root / "vendor/noVNC"
    novnc = read_json(novnc_root / "SHA256SUMS.json")
    for name in {*novnc, "SHA256SUMS.json"}:
        originals[f"build-source/noVNC/{name}"] = f"vendor/noVNC/{name}"
    generated = set()
    if platform == "windows":
        originals["install-per-user.ps1"] = "packaging/windows/install-per-user.ps1"
        generated.add("TermCraft.ico")
    else:
        originals["desktop-integration.py"] = f"packaging/{platform}/desktop-integration.py"
    if macos:
        executables.append("termcraft-desktop-launcher")
        generated.add("TermCraft.icns")
        plist = plistlib.loads((root / "packaging/macos/Info.plist").read_bytes())
        plist.update(CFBundleVersion=versions["harbor-workspace"], CFBundleShortVersionString=versions["harbor-workspace"])
        require(plistlib.loads((package / "Contents/Info.plist").read_bytes()) == plist,
                "macOS bundle metadata differs from the release identity")

    for name, source in originals.items():
        require(files.get(resources + name) == sha256(root / source), f"Missing or modified core file: {name}")

    licenses = manifest["licenses"]
    require(licenses["project"] == resources + "LICENSE" and licenses["credits"] == resources + "CREDITS.md"
            and licenses["crate_notice_paths_relative_to"] == (resources.rstrip("/") or ".")
            and isinstance(licenses["crates"], list) and licenses["crates"], "Missing core license declarations")
    require(canonical_crates(licenses["crates"]) == expected_notices["crates"],
            "Crates differ from the locked dependency notice inventory")
    notices = {resources + name: digest for name, digest in expected_notices["files"].items()}
    require(all(files.get(name) == digest for name, digest in notices.items()),
            "Packaged dependency notice paths or hashes differ from resolved sources")
    allowed = {resources + name for name in originals.keys() | generated}
    allowed |= notices.keys() | {binaries + name for name in executables}
    if macos:
        allowed.add("Contents/Info.plist")
    require(not links and set(files) == allowed, "Archive contains missing or unexpected core payload entries")

    runtime = read_json(root / "packaging/runtime-requirements.json")
    for key in ("platform_requirements", "optional_native_providers"):
        runtime[key] = runtime[key][platform]
    if platform != "linux":
        del runtime["linux_display_libraries"]
    vnc = read_json(novnc_root / "UPSTREAM.json")
    runtime.update(
        artifact_declarations="RUNTIME-MANIFEST.json",
        managed_vnc_browser={"release": vnc["release"], "source_url": vnc["source"],
                             "source_archive_sha256": vnc["sha256"], "embedded_runtime": True,
                             "source_and_notices": "build-source/noVNC"},
        linked_libraries=manifest["runtime"]["linked_libraries"],
    )
    require(runtime == manifest["runtime"] and set(runtime["linked_libraries"]) == set(executables),
            "Unsupported optional or missing core runtime declarations")
    executable_hashes = set()
    for name in executables:
        image = package / (binaries + name)
        try:
            arch = PACKAGER.binary_arch(image, platform)
        except (ValueError, subprocess.SubprocessError) as error:
            raise ValueError(f"Invalid packaged executable {name}: {error}") from error
        require(arch == TARGETS[platform][0], f"Incorrect packaged executable architecture: {name}")
        if os.name != "nt":
            require(image.stat().st_mode & 0o100, f"Missing executable permission: {name}")
        executable_hashes.add(files[binaries + name])
    require(len(executable_hashes) == len(executables), "Core executable entries must be distinct")


def verify_portable(root, archive, platform, versions, inputs, work):
    version = versions["harbor-workspace"]
    expected = portable_name(platform, version)
    require(archive.name == expected, "Unexpected portable artifact name")
    stage = work / "portable"
    (extract_tar if platform == "linux" else extract_zip)(archive, stage)
    package = stage / ("TermCraft.app" if platform == "macos" else expected.removesuffix(TARGETS[platform][2]))
    require(list(stage.iterdir()) == [package] and package.is_dir() and not package.is_symlink(),
            "Unexpected portable archive root")
    resources = package / "Contents/Resources" if platform == "macos" else package
    manifest_path = resources / "PACKAGE-MANIFEST.json"
    require(manifest_path.is_file() and not manifest_path.is_symlink(), "Missing regular package manifest")
    manifest = read_json(manifest_path)
    identity = (manifest["schema_version"], manifest["application"], manifest["version"], manifest["rdp_helper_version"],
                manifest["profile"], manifest["platform"], manifest["architecture"], manifest["signing"])
    require(identity == (1, "TermCraft", version, versions["termcraft-rdp-engine"], "release", platform,
                         TARGETS[platform][0], "unsigned") and "wayland_helper_version" not in manifest,
            "Unexpected release package identity")
    files, links = file_inventory(package, exclude=manifest_path)
    require(files == manifest["files"] and links == manifest.get("symlinks", {}),
            "Portable archive files or symlinks differ from its manifest")
    build = manifest["build"]
    require(build["provenance"] == "local locked release build" and not build.get("gui_features")
            and build["source_inputs"] == inputs and build["snapshot_sha256"] == PACKAGER.source_digest(inputs),
            "Release build input hashes differ from the source checkout")
    for tool in ("cargo", "rustc"):
        require(re.fullmatch(rf"{tool} {re.escape(RUST)}(?: \([^\r\n]+\))?", build[tool]),
                f"Unexpected release {tool} version")
    identity = {key: build[key] for key in ("cargo", "rustc", "provenance", "snapshot_sha256")}
    if platform == "macos":
        value = build["macos"]
        require(value["architecture"] == "aarch64" and value["deployment_target"] == "14.0"
                and value["compiler"].strip() and value["sdk_version"].strip(), "Invalid macOS toolchain identity")
        identity["macos"] = {key: value[key] for key in ("architecture", "deployment_target", "compiler", "sdk_version")}
    expected_notices = expected_dependency_notices(root, platform, work)
    core_layout(root, package, files, links, manifest, platform, versions, expected_notices)
    return {"name": manifest_path.relative_to(package).as_posix(),
            "sha256": sha256(manifest_path), "schema": manifest["schema_version"], "build": identity,
            "app_sha256": files[app_member(platform)],
            **({"rdp_helper_sha256": files["termcraft-rdp-engine"]} if platform == "linux" else {}),
            **({"linux_portable_files": sorted([*files, "PACKAGE-MANIFEST.json"])} if platform == "linux" else {}),
            "dependency_notices": {"sha256": PACKAGER.source_digest(expected_notices),
                                   "crates": len(expected_notices["crates"]),
                                   "files": len(expected_notices["files"])}}


def png_observation(path):
    """Read the app's 8-bit, noninterlaced RGB(A) PNG and check real color variance."""
    require(path.stat().st_size <= 24_000_000, "Startup screenshot exceeds the evidence limit")
    raw = path.read_bytes()
    require(raw[:8] == b"\x89PNG\r\n\x1a\n", "Startup screenshot is not PNG")
    offset, dimensions, data, complete = 8, None, bytearray(), False
    while offset + 12 <= len(raw):
        length, kind = struct.unpack_from(">I4s", raw, offset)
        end = offset + length + 8
        require(end + 4 <= len(raw), "Truncated startup PNG chunk")
        value = raw[offset + 8:end]
        require(zlib.crc32(kind + value) == struct.unpack_from(">I", raw, end)[0], "Invalid startup PNG CRC")
        offset = end + 4
        if kind == b"IHDR":
            require(dimensions is None and offset == 33 and length == 13, "Invalid startup PNG header")
            width, height, depth, color, compression, filt, interlace = struct.unpack(">IIBBBBB", value)
            require(640 <= width <= 4096 and 400 <= height <= 4096 and width * height <= 8_400_000
                    and depth == 8 and color in (2, 6) and compression == filt == interlace == 0,
                    "Unsupported startup PNG dimensions or format")
            dimensions = [width, height]
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            require(dimensions is not None, "Missing startup PNG header")
            data.extend(value)
        elif kind == b"IEND":
            complete = length == 0 and offset == len(raw)
            break
    require(complete and dimensions and data, "Incomplete startup PNG")
    stride = width * channels
    size = height * (stride + 1)
    inflater = zlib.decompressobj()
    pixels = inflater.decompress(data, size + 1)
    require(len(pixels) == size and inflater.eof and not inflater.unused_data, "Invalid startup PNG pixel data")

    sums, squares = [0] * 3, [0] * 3
    previous = bytearray(stride)
    for row_start in range(0, size, stride + 1):
        mode = pixels[row_start]
        require(mode <= 4, "Invalid startup PNG filter")
        row = bytearray(pixels[row_start + 1:row_start + stride + 1])
        if mode:
            for i in range(stride):
                a = row[i - channels] if i >= channels else 0
                b = previous[i]
                if mode == 1:
                    prediction = a
                elif mode == 2:
                    prediction = b
                elif mode == 3:
                    prediction = (a + b) // 2
                else:
                    c = previous[i - channels] if i >= channels else 0
                    pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                    prediction = a if pa <= min(pb, pc) else b if pb <= pc else c
                row[i] = (row[i] + prediction) & 255
        previous = row
        alpha = row[3::4] if channels == 4 else None
        blend = alpha is not None and alpha.count(255) != len(alpha)
        for channel in range(3):
            samples = row[channel::channels]
            if blend:
                samples = bytes(sample * opacity // 255 for sample, opacity in zip(samples, alpha))
            sums[channel] += sum(samples)
            squares[channel] += sum(n * n for n in samples)
    count = width * height
    require(max(math.sqrt(max(0, sq / count - (total / count) ** 2))
                for sq, total in zip(squares, sums)) >= 2, "Startup screenshot is blank")
    return dimensions


def verify_startup(root, directory, archive, digest, platform, expected_backend=None):
    entries = regular_entries(directory)
    require(set(entries) == STARTUP_FILES, "Startup evidence is incomplete or has unexpected files")
    observed = read_json(entries["verification.json"])
    backend = expected_backend or ("X11" if platform == "linux" else platform)
    require(observed["archive"] == archive and observed["archive_sha256"] == digest
            and observed["manifest_schema"] == 1 and observed["platform"] == platform
            and observed["display_backend"] == backend
            and observed["start_environment"] == "relocated package, empty PATH, isolated workspace, existing native display"
            and observed["rdp_helper_wire_version"] == 1 and observed["rdp_helper_child_location_verified"] is True
            and observed["rdp_session"] == "Received initial RDP negotiation on a loopback-only fixture; workspace preserved"
            and observed["rdp_workspace"] == ("isolated LOCALAPPDATA/Harbor default"
                                               if platform == "windows" else "explicit isolated workspace"),
            "Startup observations do not match this archive, platform and helper checks")
    expected_gl = read_json(root / "tests/windows-software-gl.json")["files"] if platform == "windows" else None
    require(observed["windows_software_gl_fixture"] == expected_gl, "Unexpected startup graphics fixture")
    screen_size = png_observation(entries["startup.png"])
    require(screen_size == observed["screen_size"], "Startup screenshot size mismatch")
    packet = entries["rdp-helper.log"].read_bytes()
    end = 5 + int.from_bytes(packet[1:5], "big")
    require(len(packet) >= end and packet[:1] == b"\x01" and end > 5
            and json.loads(packet[5:end]) == {"Ready": {"version": 1}},
            "Startup RDP helper handshake is missing or invalid")
    require(b"TermCraft remote workspace" in entries["help.log"].read_bytes(), "Missing packaged CLI startup result")
    # Raw observations are private input for backend-specific checks only. The
    # inventory gets an explicit summary plus hashes of the original evidence.
    summary = {"archive": archive, "archive_sha256": digest, "platform": platform,
               "display_backend": backend, "screen_size": screen_size,
               "rdp_helper_wire_version": 1, "rdp_helper_child_location_verified": True}
    return observed, {"observation": summary, "files": {name: sha256(path) for name, path in entries.items()}}


FIXTURE_FILES = {
    "workspace/workspace.json", "workspace/credentials.vault", "workspace/unrelated.txt",
    "workspace/remote-recovery/qualification/draft.txt",
    "malformed workspace/workspace.json", "malformed workspace/credentials.vault",
}
FIXTURE_PROOF = (b"Package fixture verified: store, shortcuts, recovery, malformed originals, "
                 b"vault plaintext exclusion and decryption.")
LIFECYCLE_STAGES = {
    "linux": (("deb-install-baseline", "baseline", True), ("deb-upgrade-candidate", "candidate", True),
              ("deb-reinstall-candidate", "candidate", True), ("deb-purge", "candidate", False),
              ("desktop-start-candidate", "candidate", True), ("desktop-remove", "candidate", False)),
    "windows": (("windows-install-baseline", "baseline", True), ("windows-install-candidate", "candidate", True),
                ("windows-remove-baseline", "baseline", False), ("windows-remove-candidate", "candidate", False),
                ("windows-reinstall-candidate", "candidate", True), ("windows-final-remove", "candidate", False)),
    "macos": (("mac-install-baseline", "baseline", True), ("mac-replace-candidate", "candidate", True),
              ("mac-relocate-candidate", "candidate", True), ("mac-unregister-remove", "candidate", False)),
}
DEBIAN_PATHS = {"/opt/termcraft", "/usr/bin/termcraft", "/usr/share/applications/org.termcraft.TermCraft.desktop",
                "/usr/share/pixmaps/termcraft.png", "/usr/share/doc/termcraft"}


def app_member(platform):
    return {"linux": "harbor-workspace", "windows": "harbor-workspace.exe",
            "macos": "Contents/MacOS/harbor-workspace"}[platform]


def measured_sha256(value):
    # Repeated-character placeholders are not observations of measured bytes.
    return isinstance(value, str) and SHA256.fullmatch(value) is not None and len(set(value)) > 1


def verify_lifecycle_operations(platform, stages, roles, manifest):
    def removal(name):
        paths = stages[name]["paths"]["removed"]
        require(isinstance(paths, list) and len(paths) == len(set(paths)), "Invalid lifecycle removal list")
        return {str(PureWindowsPath(path)) if platform == "windows" else path for path in paths}

    def package(name):
        return stages[name]["paths"]["package"]

    def registration(name):
        return stages[name]["registration"]

    if platform == "linux":
        for name, role, _ in LIFECYCLE_STAGES["linux"][:3]:
            require(package(name) == "/opt/termcraft" and stages[name]["launcher"] == ["/usr/bin/termcraft"]
                    and registration(name) == {"target": "/usr/bin/termcraft", "version": roles[role]["version"]},
                    "Debian installed version or launcher evidence differs")
        require(removal("deb-purge") == DEBIAN_PATHS, "Debian purge does not cover owned package paths")
        start, finish = stages["desktop-start-candidate"], stages["desktop-remove"]["paths"]
        require(len(start["launcher"]) == 2, "Portable desktop launcher must use explicit argv")
        posix = PurePosixPath
        launcher = posix(start["launcher"][1])
        data = launcher.parents[2]
        started = posix(start["paths"]["package"])
        require(launcher == data / "termcraft/desktop/launch.py"
                and str(started) == str(launcher.parent / "payload")
                and registration("desktop-start-candidate") == {
                    "target": str(data / "applications/org.termcraft.TermCraft.desktop"),
                    "receipt": str(launcher.parent / "owned.json")}
                and posix(start["paths"]["original_absent"]).is_absolute()
                and posix(start["paths"]["original_absent"]) != started
                and {str(launcher), *(str(started / name) for name in manifest["linux_portable_files"]),
                     str(data / "icons/hicolor/256x256/apps/org.termcraft.TermCraft.png"),
                     str(data / "mime/packages/org.termcraft.TermCraft.xml"),
                     *registration("desktop-start-candidate").values()} == removal("desktop-remove")
                and finish["unrelated_before"] == finish["unrelated_after"]
                and set(finish["unrelated_before"]) == {"mimeapps.list", "unrelated.desktop"}
                and all(measured_sha256(value) for value in finish["unrelated_before"].values()),
                "Portable desktop installation or removal evidence differs")
    elif platform == "windows":
        def owned(name, role):
            path = PureWindowsPath(package(name))
            shortcut = PureWindowsPath(registration(name)["shortcut"])
            version = roles[role]["version"] + "-x86_64"
            require(path.name == version and path.parent.name == "TermCraft" and path.parent.parent.name == "Programs"
                    and shortcut.is_absolute() and shortcut.name == "TermCraft " + version + ".lnk"
                    and PureWindowsPath(registration(name)["target"]) == path / app_member("windows")
                    and PureWindowsPath(registration(name)["working_directory"]) == path
                    and stages[name]["launcher"] == [registration(name)["target"]],
                    "Installed Windows KnownFolder or Start menu observation differs")
            return {str(path), str(shortcut)}

        base = owned("windows-install-baseline", "baseline")
        new = owned("windows-install-candidate", "candidate")
        require(not base & new and base == removal("windows-remove-baseline")
                and new == {str(PureWindowsPath(path)) for path in stages["windows-remove-baseline"]["paths"]["retained"]}
                and new == removal("windows-remove-candidate") == owned("windows-reinstall-candidate", "candidate")
                == removal("windows-final-remove"), "Windows side-by-side cleanup or reinstall differs")
    else:
        install, replace, relocate = ("mac-install-baseline", "mac-replace-candidate", "mac-relocate-candidate")
        initial, final = PurePosixPath(package(install)), PurePosixPath(package(relocate))
        receipt = registration(install)["receipt"]
        for name in (install, replace, relocate):
            require(registration(name) == {"target": package(name), "receipt": receipt}
                    and stages[name]["launcher"] == [str(PurePosixPath(package(name)) / "Contents/MacOS/termcraft-desktop-launcher")],
                    "macOS installed bundle or registration observation differs")
        removed = stages["mac-unregister-remove"]["paths"]
        require(initial == PurePosixPath(package(replace)) and initial != final and initial.parent == final.parent
                and initial.name == "TermCraft.app" and final.suffix == ".app"
                and all(char in final.name for char in (" ", "'", "%"))
                and PurePosixPath(receipt).is_absolute()
                and removal("mac-unregister-remove") == {str(initial), str(final), receipt}
                and type(removed["unrelated_registrations"]) is int and removed["unrelated_registrations"] > 0
                and measured_sha256(removed["unrelated_before"])
                and removed["unrelated_before"] == removed["unrelated_after"],
                "macOS relocation or LaunchServices cleanup evidence differs")


def verify_native_evidence(root, lifecycle, wayland, platform, source, version, artifacts, manifest, inputs):
    require(lifecycle is not None and lifecycle.is_dir() and not lifecycle.is_symlink(), "Lifecycle evidence is required")
    entries = regular_entries(lifecycle)
    portable = portable_name(platform, version)
    value = read_json(entries["verification.json"])
    require(value["schema"] == 2 and value["source_sha"] == source and value["platform"] == platform
            and value["architecture"] == TARGETS[platform][0] and value["version"] == version
            and value["archive_sha256"] == artifacts[portable]
            and value["fixture_source_sha256"] == sha256(root / "tests/package_workspace_fixture.rs"),
            "Lifecycle evidence does not bind the source, package and fixture")
    roles = value["roles"]
    baseline, candidate = roles["baseline"], roles["candidate"]
    old = previous_version(version)
    require(set(roles) == {"baseline", "candidate"} and candidate == {
        "archive": portable, "archive_sha256": artifacts[portable], "version": version,
        "snapshot_sha256": manifest["build"]["snapshot_sha256"], "manifest_sha256": manifest["sha256"],
        "app_sha256": manifest["app_sha256"], "embedded_version": f"TermCraft {version}"},
        "Lifecycle candidate differs from the verified archive")
    manifest_old = read_json(entries["baseline-manifest.json"])
    patch = {}
    for name in ("Cargo.toml", "Cargo.lock"):
        original = (root / name).read_bytes()
        patched = patched_version_bytes(name, original, version, old)
        patch[name] = {"before": hashlib.sha256(original).hexdigest(), "after": hashlib.sha256(patched).hexdigest()}
    old_inputs = {**inputs, **{name: item["after"] for name, item in patch.items()}}
    require(value["private_version_patch"] == {"from": version, "to": old, "files": patch}
            and manifest_old["build"]["source_inputs"] == old_inputs
            and manifest_old["build"]["snapshot_sha256"] == PACKAGER.source_digest(old_inputs)
            and manifest_old["build"]["provenance"] == "local locked release build"
            and manifest_old["profile"] == "release" and manifest_old["platform"] == platform
            and manifest_old["architecture"] == TARGETS[platform][0] and manifest_old["signing"] == "unsigned"
            and manifest_old["rdp_helper_version"] == package_versions(root)["termcraft-rdp-engine"]
            and manifest_old["version"] == old and baseline == {
                "archive": portable_name(platform, old), "archive_sha256": baseline["archive_sha256"], "version": old,
                "snapshot_sha256": manifest_old["build"]["snapshot_sha256"],
                "manifest_sha256": sha256(entries["baseline-manifest.json"]),
                "app_sha256": manifest_old["files"][app_member(platform)], "embedded_version": f"TermCraft {old}"}
            and all(measured_sha256(baseline[key]) and baseline[key] != candidate[key]
                    for key in ("archive_sha256", "snapshot_sha256", "app_sha256")),
            "Lifecycle baseline does not prove an independently versioned exact source")
    if platform == "linux":
        debian = value["debian"]
        require(set(debian) == {"baseline", "candidate"} and all(
            debian[role]["name"] == f"termcraft_{roles[role]['version']}_amd64.deb"
            and debian[role]["version"] == roles[role]["version"]
            and measured_sha256(debian[role]["sha256"]) for role in roles)
            and debian["candidate"]["sha256"] == artifacts[debian["candidate"]["name"]]
            and debian["baseline"]["sha256"] != debian["candidate"]["sha256"],
            "Debian lifecycle does not match the candidate and lower version")
    else:
        require(value["debian"] is None and wayland is None, "Unexpected Linux lifecycle evidence")
    expected_files = {"verification.json", "baseline-manifest.json", "fixture-seed.log"}
    fixture = value["fixture_before"]
    require(set(fixture) == FIXTURE_FILES and all(measured_sha256(digest) for digest in fixture.values())
            and FIXTURE_PROOF in entries["fixture-seed.log"].read_bytes(), "Initial workspace/vault fixture is missing")
    specs = LIFECYCLE_STAGES[platform]
    require([(item["stage"], item["role"]) for item in value["stages"]] ==
            [(name, role) for name, role, _ in specs], "Lifecycle required stages were skipped or reordered")
    pure = PureWindowsPath if platform == "windows" else PurePosixPath
    fixture_root, evidence_origin = pure(value["fixture_root"]), pure(value["evidence_origin"])
    require(fixture_root.is_absolute() and fixture_root.name == "fixtures" and evidence_origin.is_absolute()
            and ".." not in fixture_root.parts and ".." not in evidence_origin.parts
            and not (fixture_root == evidence_origin or fixture_root in evidence_origin.parents
                     or evidence_origin in fixture_root.parents),
            "Invalid workspace root or origin for lifecycle launch records")
    for (name, role, starts), stage in zip(specs, value["stages"], strict=True):
        log = f"{name}-fixture.log"
        expected_files.add(log)
        require(log in entries and FIXTURE_PROOF in entries[log].read_bytes() and stage["preserved"] == fixture,
                f"Lifecycle workspace, vault or recovery check missing or changed: {name}")
        if starts:
            launched = roles[role]
            package = pure(stage["paths"]["package"])
            require(package.is_absolute() and pure(stage["executable"]) == package / app_member(platform)
                    and stage["app_sha256"] == launched["app_sha256"]
                    and stage["embedded_version"] == launched["embedded_version"]
                    and stage["launcher"] and all(pure(part).is_absolute() for part in stage["launcher"]),
                    f"Lifecycle executing path or binary does not match the {role} package")
            for label, workspace_name in (("workspace", "workspace"), ("malformed", "malformed workspace")):
                stem = f"{name}-{label}"
                png, run_log, record = stem + ".png", stem + ".log", stem + "-launch.json"
                expected_files.update((png, run_log, record))
                require(png in entries and stage[label] == {"name": png, "sha256": sha256(entries[png]),
                                                            "size": png_observation(entries[png])},
                        f"Lifecycle screenshot is absent or does not match: {name}/{label}")
                require(record in entries and run_log in entries, f"Missing lifecycle launch record or output: {stem}")
                try:
                    observed_launch = read_json(entries[record])
                except ValueError as error:
                    raise ValueError(f"Invalid lifecycle launch record: {stem}") from error
                require(isinstance(observed_launch, dict)
                        and type(observed_launch.get("exit_code")) is int
                        and type(observed_launch.get("screenshot_preexisting")) is bool
                        and observed_launch == {
                    "stage": name, "role": role,
                    "executable": stage["executable"], "executable_sha256": launched["app_sha256"],
                    "argv": [*stage["launcher"], "--data-dir", str(fixture_root / workspace_name),
                             "--screenshot", str(evidence_origin / png)],
                    "exit_code": 0, "screenshot_preexisting": False, "screenshot": stage[label],
                    "output_sha256": sha256(entries[run_log]),
                }, f"Lifecycle launch record does not bind executable, workspace and new screenshot: {stem}")
    require(set(entries) == expected_files, "Lifecycle evidence files are incomplete or excessive")
    verify_lifecycle_operations(platform, {stage["stage"]: stage for stage in value["stages"]}, roles, manifest)
    summary = {"schema": 2, "source_sha": source, "platform": platform, "architecture": TARGETS[platform][0],
               "version": version, "archive_sha256": artifacts[portable],
               "fixture_source_sha256": value["fixture_source_sha256"],
               "roles": {role: {key: info[key] for key in ("version", "archive_sha256", "snapshot_sha256",
                                                          "manifest_sha256", "app_sha256")} for role, info in roles.items()},
               "stages": [{"stage": name, "role": role, "startup_verified": starts,
                           "fixture_preserved": True} for name, role, starts in specs]}
    result = {"lifecycle": {"observation": summary, "files": {name: sha256(path) for name, path in entries.items()}}}
    if platform == "linux":
        require(wayland is not None and wayland.is_dir(), "Native Wayland evidence is required")
        observed, check = verify_startup(root, wayland, portable, artifacts[portable], platform, expected_backend="Wayland")
        verify_wayland(observed, wayland, source)
        check["observation"]["actual_weston_connection_verified"] = True
        result["wayland"] = check
    return result


def verify_wayland(obs, wayland, source):
    weston = obs["weston"]
    socket = weston["socket"]
    trace = (wayland / "startup.log").read_bytes()
    require(obs["source_sha"] == source and weston["backend"] == "headless"
            and weston["display_absent"] is True and weston["wayland_socket_absent"] is True
            and weston["runtime_mode"] == "0700" and type(weston["pid"]) is int and weston["pid"] > 1
            and type(weston["socket_inode"]) is int and weston["socket_inode"] > 0
            and PurePosixPath(socket).is_absolute() and PurePosixPath(socket).name == "termcraft-release-wayland"
            and STARTUP.successful_wayland_connect(trace, socket), "Native Wayland provenance or actual Weston socket connection missing")



def verify_appimage_evidence(root, directory, source, entries, portable, version, manifest, work):
    name, corresponding = APPIMAGE.names(version)
    info, _ = APPIMAGE.verify_image(entries[portable], entries[name], entries[corresponding], work)
    checks = {}
    for backend in ("X11", "Wayland"):
        location = directory / f"appimage-{backend.lower()}-evidence"
        observed, check = verify_startup(root, location, name, sha256(entries[name]), "linux", expected_backend=backend)
        if backend == "Wayland":
            verify_wayland(observed, location, source)
            check["observation"]["actual_weston_connection_verified"] = True
        obs = observed["appimage"]
        mapped = obs["mapped_private_libraries"]
        require(isinstance(mapped, list) and all(isinstance(lib, str) for lib in mapped)
                and mapped == sorted(set(mapped)) and set(mapped) <= set(info["libraries"])
                and ("libwayland-client.so.0" if backend == "Wayland" else "libX11.so.6") in mapped,
                "Missing actual AppImage private client-library identity")
        evidence = (location / "rdp-session.log").read_bytes()
        packets = re.findall(rb"^\[AppImage loopback X\.224/RDP negotiation\] ([0-9a-f]{38,8192})$", evidence, re.M)
        require(len(packets) == 1, "Missing unique initial RDP negotiation observation")
        packet = bytes.fromhex(packets[0].decode("ascii"))
        require(packet[:2] == b"\x03\x00" and packet == STARTUP.initial_rdp_request(packet[:4], io.BytesIO(packet[4:])),
                "Invalid initial AppImage RDP negotiation")
        overlap = obs["concurrent_extractions"]
        directories, inodes = overlap["directories"], overlap["inodes"]
        require(set(overlap) == {"directories", "inodes", "uid", "private_mode", "shared_parent_mode",
                                 "surviving_gui_and_files_verified", "both_extractions_removed"}
                and isinstance(directories, list) and len(directories) == 2
                and all(re.fullmatch(r"appimage_extracted_[A-Za-z0-9]{6}", name) for name in directories)
                and directories[0] != directories[1] and isinstance(inodes, list) and len(inodes) == 2
                and all(type(inode) is int and inode > 0 for inode in inodes) and inodes[0] != inodes[1]
                and type(overlap["uid"]) is int and overlap["uid"] > 0
                and overlap["private_mode"] == "0700" and overlap["shared_parent_mode"] == "1777"
                and overlap["surviving_gui_and_files_verified"] is True and overlap["both_extractions_removed"] is True,
                "Overlapping AppImage invocations lacked distinct private extraction and cleanup")
        recorded = re.findall(rb"^\[AppImage concurrent runtime observations\] (.+)$",
                              (location / "startup.log").read_bytes(), re.M)
        require(recorded == [json.dumps(overlap, sort_keys=True).encode("ascii")], "Missing AppImage overlap observation log")
        require(obs == {"mode": "extract-and-run", "core_sha256": info["core"]["sha256"],
                        "runtime_sha256": info["runtime"]["sha256"], "source_sha256": info["corresponding_source"]["sha256"],
                        "highest_required_glibc": info["highest_required_glibc"], "gui_sha256": manifest["app_sha256"],
                        "helper_sha256": manifest["rdp_helper_sha256"], "mapped_private_libraries": mapped,
                        "negotiation_sha256": hashlib.sha256(packet).hexdigest(),
                        "runtime_reaped_and_clean": True, "concurrent_extractions": overlap} and obs["runtime_reaped_and_clean"] is True,
                "AppImage launch evidence differs from the inspected outer image, source, GUI or helper")
        check["observation"]["appimage"] = {
            "mode": "extract-and-run", "mapped_private_libraries": mapped,
            "negotiation_sha256": hashlib.sha256(packet).hexdigest(),
            "runtime_reaped_and_clean": True, "private_concurrent_extractions_verified": True,
        }
        checks[backend] = check
    return {"manifest": info, "startup": checks}


def verify_candidate(root, source, version, platform, dist, startup, inputs, lifecycle=None, wayland=None):
    require(platform in TARGETS and SHA1.fullmatch(source), "Invalid platform or candidate source commit")
    versions = package_versions(root)
    require(versions["harbor-workspace"] == version, "Candidate application version mismatch")
    portable = portable_name(platform, version)
    names = {portable}
    if platform == "linux":
        names.add(f"termcraft_{version}_amd64.deb")
        names.update(APPIMAGE.names(version))
    entries = regular_entries(dist)
    require(set(entries) == names, "Missing or unexpected candidate artifact filenames")
    artifacts = {name: sha256(path) for name, path in entries.items()}
    with tempfile.TemporaryDirectory(prefix="termcraft-verify-") as temporary:
        work = Path(temporary).resolve()
        manifest = verify_portable(root, entries[portable], platform, versions, inputs, work)
        appimage = {}
        if platform == "linux":
            # The repository builder emits a deterministic envelope including
            # the portable payload and both internal manifests. Comparing it
            # checks control metadata, launcher, policy copies and every byte.
            deb = DEBIAN.package_debian(entries[portable], work / "deb")
            require(sha256(deb) == artifacts[deb.name], "Debian package differs from its portable source or own manifest")
            debian = {"name": deb.name, "portable_archive_sha256": artifacts[portable]}
            appimage["appimage"] = verify_appimage_evidence(root, startup.parent, source, entries, portable, version,
                                                            manifest, work / "appimage")
        else:
            debian = None
    _, observation = verify_startup(root, startup, portable, artifacts[portable], platform)
    native = verify_native_evidence(root, lifecycle, wayland, platform, source, version, artifacts, manifest, inputs)
    return {"schema": 1, "source_sha": source, "version": version, "package_versions": versions,
            "scope": SCOPE, "platform": platform, "architecture": TARGETS[platform][0],
            "artifacts": artifacts, "package_manifest": manifest, "debian": debian,
            "checks_passed": ["portable-contents", "locked-dependency-notices", "native-startup",
                              *(["debian-portable-match", "linux-appimage-x11-wayland-source"] if debian else [])],
            "startup": observation, **native, **appimage}


def stage_candidate(root, source, version, platform, dist, startup, output, lifecycle=None, wayland=None):
    inputs = PACKAGER.source_inputs(root)
    result = verify_candidate(root, source, version, platform, dist, startup, inputs, lifecycle, wayland)
    output.mkdir(parents=True)
    shutil.copytree(dist, output / "dist")
    shutil.copytree(startup, output / "startup-evidence")
    shutil.copytree(lifecycle, output / "lifecycle-evidence")
    if wayland is not None:
        shutil.copytree(wayland, output / "wayland-evidence")
        for backend in ("x11", "wayland"):
            name = f"appimage-{backend}-evidence"
            shutil.copytree(startup.parent / name, output / name)
    write_json(output / "candidate.json", result)
    return result


def git_source_archive(root, source, output):
    with tempfile.TemporaryDirectory(prefix="termcraft-source-") as temporary:
        tar = Path(temporary) / "tracked.tar"
        subprocess.run(["git", "-C", str(root), "archive", "--format=tar",
                        f"--prefix=TermCraft-source-{source}/", f"--output={tar}", source], check=True)
        with tar.open("rb") as raw, output.open("xb") as file, gzip.GzipFile(filename="", mode="wb", fileobj=file, mtime=0) as compressed:
            shutil.copyfileobj(raw, compressed)


def checksum_text(artifacts):
    return "".join(f"{digest}  {name}\n" for name, digest in sorted(artifacts.items()))


def assemble(root, source, version, candidates, output, regression=None):
    inventory = {}
    for directory in candidates:
        require(not directory.is_symlink(), "Candidate directory is a symlink")
        path = directory / "candidate.json"
        require(not path.is_symlink() and path.is_file(), "Missing regular candidate.json")
        value = read_json(path)
        platform = value["platform"]
        require(platform in TARGETS and platform not in inventory,
                "Expected exactly one inventory for each platform")
        require(directory.name == f"candidate-{TARGETS[platform][1]}" and
                {item.name for item in directory.iterdir()} == ({"candidate.json", "dist", "startup-evidence", "lifecycle-evidence"} |
                 ({"wayland-evidence", "appimage-x11-evidence", "appimage-wayland-evidence"} if platform == "linux" else set())),
                "Unexpected candidate artifact layout")
        inventory[platform] = (value, directory)
    require(set(inventory) == set(TARGETS), "Expected exactly one inventory for each platform")
    artifacts = {}
    checked_platforms = {}
    inputs = PACKAGER.source_inputs(root)
    for platform, (value, directory) in sorted(inventory.items()):
        require(value["source_sha"] == source and value["version"] == version, "Candidate release source or version mismatch")
        entries = regular_entries(directory / "dist")
        require(set(entries) == set(value["artifacts"]) and all(
            sha256(file) == value["artifacts"][name] for name, file in entries.items()),
            f"Downloaded {platform} asset hashes differ from its inventory")
        checked = verify_candidate(root, source, version, platform, directory / "dist",
                                   directory / "startup-evidence", inputs, directory / "lifecycle-evidence",
                                   directory / "wayland-evidence" if platform == "linux" else None)
        require(value == checked, f"Downloaded {platform} candidate inventory does not match its content")
        checked_platforms[platform] = checked
        for name, digest in checked["artifacts"].items():
            require(SAFE_NAME.fullmatch(name) and name not in artifacts, f"Unsafe or duplicate release asset: {name}")
            artifacts[name] = digest

    regression_ok = (isinstance(regression, dict) and regression.get("result") == "success"
                     and regression.get("workflow") == ".github/workflows/ci.yml"
                     and all(re.fullmatch(r"[1-9][0-9]*", regression.get(key, ""))
                             for key in ("run_id", "run_attempt")))
    # Do not copy optional/runtime diagnostic outputs or raw runner logs into
    # the public asset set. Their separately uploaded candidate evidence is
    # identified by exact hash and preserved in the aggregate inventory.
    output.mkdir(parents=True)
    for value, directory in inventory.values():
        for name, digest in value["artifacts"].items():
            shutil.copyfile(directory / "dist" / name, output / name)
            require(sha256(output / name) == digest, f"Candidate changed during assembly: {name}")
    source_name = f"TermCraft-source-{source}.tar.gz"
    git_source_archive(root, source, output / source_name)
    artifacts[source_name] = sha256(output / source_name)
    result = {
        "schema": 1, "source_sha": source, "version": version, "scope": SCOPE,
        "source": {"archive": source_name, "sha256": artifacts[source_name], "commit": source,
                   "kind": "git archive of tracked project files", "project_license": "LICENSE",
                   "credits": "CREDITS.md", "privacy": "PRIVACY.md",
                   "rdp_source": "engines/rdp", "wayland_source": "engines/wayland",
                   "notice": "Dependency locks and build-input hashes are not a plugin or dependency source bundle."},
        "artifacts": artifacts, "platforms": checked_platforms,
        "checks_passed": ["three-platform-candidate-validation", *PENDING_GATES,
                          *(["three-platform-regression"] if regression_ok else [])],
        "regression": regression,
        "pending_gates": [] if regression_ok else ["three-platform-regression"], "publication_ready": regression_ok,
    }
    write_json(output / "release-inventory.json", result)
    checksum_entries = {**artifacts, "release-inventory.json": sha256(output / "release-inventory.json")}
    (output / "SHA256SUMS").write_text(checksum_text(checksum_entries), encoding="ascii", newline="\n")
    return result


def publish(github, event, ref, version, source, ready, paths):
    require(publish_event(event, ref, version) and SHA1.fullmatch(source) and ready is True,
            "Public publication requires a qualified exact version tag and source commit")
    expected = {}
    for path in paths:
        name = path.name
        require(SAFE_NAME.fullmatch(name) and name not in expected and path.is_file() and not path.is_symlink(),
                f"Unsafe or duplicate publication asset: {name}")
        expected[name] = f"sha256:{sha256(path)}"
    require(expected, "No release assets to publish")
    tag = f"v{version}"
    require(github.tag_commit(tag) == source, "Remote version tag points at another source commit")
    release = github.release_for_tag(tag)
    created = release is None
    if created:
        github.create_draft(tag, source, sorted(paths))
        release = github.release_for_tag(tag)
        require(release is not None and release["draft"] is True, "Expected the newly created draft release")

    assets = {}
    for asset in github.release_assets(release["id"]):
        name, digest = asset["name"], asset["digest"]
        require(SAFE_NAME.fullmatch(name) and name not in assets and asset["state"] == "uploaded"
                and isinstance(digest, str) and digest.startswith("sha256:")
                and SHA256.fullmatch(digest.removeprefix("sha256:")), "Existing release contains unverifiable assets")
        assets[name] = digest
    require(assets == expected, "Existing release asset names or hashes differ. No release assets were overwritten.")
    if created:
        require(github.tag_commit(tag) == source, "Remote version tag changed while preparing release")
        github.publish_draft(release["id"])
    else:
        require(release["draft"] is False, "An existing draft is never published automatically on retry")


class GitHub:
    def __init__(self, repo):
        require(REPO.fullmatch(repo), "Expected an explicit owner/repository")
        self.repo = repo

    def invoke(self, *args):
        return subprocess.check_output(["gh", *args])

    def api(self, path, *options):
        return json.loads(self.invoke("api", "--hostname", "github.com", *options, f"repos/{self.repo}/{path}"))

    def pages(self, path):
        return (item for page in self.api(path, "--paginate", "--slurp") for item in page)

    def tag_commit(self, tag):
        ref = self.api(f"git/ref/tags/{tag}")
        require(ref["ref"] == f"refs/tags/{tag}", "GitHub returned a different release ref")
        obj = ref["object"]
        for _ in range(8):
            kind, sha = obj["type"], obj["sha"]
            require(SHA1.fullmatch(sha), "Invalid GitHub tag target")
            if kind == "commit":
                return sha
            require(kind == "tag", "Release tag does not resolve to a commit")
            obj = self.api(f"git/tags/{sha}")["object"]
        raise ValueError("Tag nesting depth exceeded")

    def release_for_tag(self, tag):
        # Listing includes drafts for the actor with contents:write. Treat API
        # failures as failures, never infer a missing release from any error.
        releases = [release for release in self.pages("releases?per_page=100") if release["tag_name"] == tag]
        require(len(releases) <= 1, "Multiple GitHub releases use this tag")
        return releases[0] if releases else None

    def release_assets(self, identifier):
        require(type(identifier) is int and identifier > 0, "Invalid release ID")
        return self.pages(f"releases/{identifier}/assets?per_page=100")

    def create_draft(self, tag, source, paths):
        self.invoke("release", "create", tag, "--repo", f"github.com/{self.repo}", "--verify-tag", "--target", source,
                    "--draft", "--title", f"TermCraft {tag}", "--notes",
                    "Unsigned TermCraft core desktop packages. See release-inventory.json and SHA256SUMS "
                    "for exact assets, scope and source. Optional runtime payloads are not included.",
                    "--", *(str(path.resolve()) for path in paths))

    def publish_draft(self, identifier):
        require(type(identifier) is int and identifier > 0, "Invalid release ID")
        result = self.api(f"releases/{identifier}", "--method", "PATCH", "--field", "draft=false")
        require(result["draft"] is False, "GitHub did not publish the verified draft")


def publication_files(directory, source, version):
    entries = regular_entries(directory)
    inventory = read_json(entries["release-inventory.json"])
    require(inventory["scope"] == SCOPE and inventory["source_sha"] == source and inventory["version"] == version
            and inventory["publication_ready"] is True and inventory["pending_gates"] == []
            and set(PENDING_GATES).issubset(inventory["checks_passed"])
            and {"three-platform-regression", "three-platform-candidate-validation"}.issubset(inventory["checks_passed"]),
            "Release inventory is not publication-ready")
    expected_assets = {portable_name(platform, version) for platform in TARGETS}
    source_archive = f"TermCraft-source-{source}.tar.gz"
    expected_assets |= {f"termcraft_{version}_amd64.deb", source_archive, *APPIMAGE.names(version)}
    artifacts = inventory["artifacts"]
    require(set(artifacts) == expected_assets and set(entries) == (expected_assets | {"release-inventory.json", "SHA256SUMS"})
            and inventory["source"]["archive"] == source_archive and inventory["source"]["commit"] == source
            and inventory["source"]["sha256"] == artifacts[source_archive],
            "Downloaded publication assets do not match the release inventory")
    require(all(SHA256.fullmatch(digest) and sha256(entries[name]) == digest for name, digest in artifacts.items()),
            "Downloaded publication asset hashes differ")
    checksums = {**artifacts, "release-inventory.json": sha256(entries["release-inventory.json"])}
    require(entries["SHA256SUMS"].read_bytes() == checksum_text(checksums).encode("ascii"),
            "Downloaded publication checksums differ")
    return list(entries.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("preflight", "inventory", "aggregate", "publish"):
        command = commands.add_parser(name)
        command.add_argument("--source", required=True)
        if name in {"preflight", "publish"}:
            command.add_argument("--event", required=True)
            command.add_argument("--ref", required=True)
        if name in {"inventory", "aggregate", "publish"}:
            command.add_argument("--version", required=True)
        if name in {"preflight", "aggregate"}:
            command.add_argument("--github-output", type=Path)
        if name in {"inventory", "aggregate", "publish"}:
            command.add_argument("--output", type=Path, required=True)
        if name == "inventory":
            command.add_argument("--platform", choices=TARGETS, required=True)
            command.add_argument("--dist", type=Path, required=True)
            command.add_argument("--startup", type=Path, required=True)
            command.add_argument("--lifecycle", type=Path, required=True)
            command.add_argument("--wayland", type=Path)
        if name == "aggregate":
            command.add_argument("--candidates", type=Path, required=True)
            command.add_argument("--regression-result", required=True)
            command.add_argument("--run-id", required=True)
            command.add_argument("--run-attempt", required=True)
        if name == "publish":
            command.add_argument("--repo", required=True)

    args = parser.parse_args()
    root = args.root.resolve()
    try:
        if args.command == "preflight":
            version = version_for(root, args.event, args.ref)
            verify_checkout(root, args.source, version)
            result = {"version": version, "source_sha": args.source,
                      "tag_publish_event": str(publish_event(args.event, args.ref, version)).lower()}
        elif args.command == "inventory":
            verify_checkout(root, args.source, args.version)
            result = stage_candidate(root, args.source, args.version, args.platform, args.dist, args.startup, args.output, args.lifecycle, args.wayland)
        elif args.command == "aggregate":
            verify_checkout(root, args.source, args.version)
            require(args.regression_result == "success" and all(re.fullmatch(r"[1-9][0-9]*", item)
                    for item in (args.run_id, args.run_attempt)), "Required platform regression workflow did not succeed")
            result = assemble(root, args.source, args.version, sorted(args.candidates.iterdir()), args.output,
                              regression={"workflow": ".github/workflows/ci.yml", "result": args.regression_result,
                                          "run_id": args.run_id, "run_attempt": args.run_attempt})
        else:
            # Recheck the event before trusting assembled inputs or contacting GitHub.
            require(publish_event(args.event, args.ref, args.version), "Manual or mismatched tag publication is forbidden")
            verify_checkout(root, args.source, args.version)
            paths = publication_files(args.output, args.source, args.version)
            publish(GitHub(args.repo), args.event, args.ref, args.version, args.source, True, paths)
            return
        if getattr(args, "github_output", None):
            keys = ("version", "source_sha", "tag_publish_event") if args.command == "preflight" else ("publication_ready",)
            with args.github_output.open("a", encoding="utf-8") as stream:
                stream.writelines(f"{key}={str(result[key]).lower()}\n" for key in keys)
        if args.command == "aggregate":
            print("Validated unsigned assembly with native lifecycle and Wayland evidence.")
        else:
            print(f"{args.command}: {result['version']} from {result['source_sha']}")
    except (OSError, ValueError, KeyError, TypeError, AttributeError, IndexError, tarfile.TarError, zipfile.BadZipFile,
            subprocess.SubprocessError, zlib.error) as error:
        parser.exit(1, f"Release {args.command} failed: {error}\n")


if __name__ == "__main__":
    main()
