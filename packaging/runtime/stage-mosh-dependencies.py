#!/usr/bin/env python3
"""Stage only exact pinned Mosh dependencies from authenticated archive inputs."""
import hashlib, io, json, shutil, subprocess, tarfile
from pathlib import Path

def digest(data):
    return hashlib.sha256(data).hexdigest()

def input_file(cache, name, expected):
    path = cache / name
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 512 * 1024 * 1024:
        raise ValueError("Expected bounded regular Mosh dependency input: " + name)
    data = path.read_bytes()
    if digest(data) != expected:
        raise ValueError("Mosh dependency input checksum mismatch: " + name)
    return data

def member(tar, name, expected, size=None):
    entry = tar.getmember(name)
    if not entry.isfile() or not 0 <= entry.size <= 128 * 1024 * 1024:
        raise ValueError("Expected bounded regular dependency member: " + name)
    data = tar.extractfile(entry).read()
    if digest(data) != expected or (size is not None and len(data) != size):
        raise ValueError("Dependency member checksum mismatch: " + name)
    return data

def output(root, name, data, executable=False):
    relative = Path(name)
    if relative.is_absolute() or any(p in ("..", ".") for p in relative.parts):
        raise ValueError("Invalid pinned output path")
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(data)
    path.chmod(0o755 if executable else 0o644)

def portable_source_basename(name):
    # Escape unsafe UTF-8 bytes and the escape marker itself. In particular,
    # Arch's epoch separator (:) must never become a portable TAR path byte.
    safe = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._+-"
    if not name or name in (".", ".."):
        raise ValueError("Invalid pinned source basename")
    return "".join(chr(byte) if byte in safe else f"~{byte:02X}" for byte in name.encode("utf-8"))

def stage(manifest_path, destination, cache):
    manifest_bytes = manifest_path.read_bytes()
    info = json.loads(manifest_bytes)
    if info["schema_version"] != 1 or info["platform"] != "linux" or info["architecture"] != "x86_64":
        raise ValueError("Unsupported Mosh dependency manifest")
    runtime = destination / info["root"]
    runtime.mkdir(parents=True, exist_ok=False)
    if len(info["files"]) > 4096 or sum(e["size"] for e in info["files"]) > 256 * 1024 * 1024:
        raise ValueError("Mosh dependency inventory exceeds bounds")
    sources = {}
    for package in info["packages"]:
        raw = input_file(cache, package["filename"], package["archive_sha256"])
        sig = input_file(cache, package["filename"] + ".sig", package["signature_sha256"])
        source = input_file(cache, package["source_filename"], package["source_sha256"])
        recipe = input_file(cache, package["recipe_filename"], package["recipe_sha256"])
        # The pins were established after detached-signature verification. Staging
        # is offline and accepts exactly those authenticated bytes.
        unpacked = subprocess.run(["zstd", "-dc"], input=raw, stdout=subprocess.PIPE, check=True).stdout
        with tarfile.open(fileobj=io.BytesIO(unpacked)) as tar:
            for entry in info["files"]:
                if entry["package"] == package["name"]:
                    data = member(tar, entry["archive_path"], entry["sha256"], entry["size"])
                    output(runtime, entry["path"], data, entry["executable"])
            for name, expected in package["inventory"].items():
                data = member(tar, name, expected)
                output(runtime, "notices/" + package["name"] + "/" + name.lstrip("."), data)
            for entry in package.get("notices", []):
                data = member(tar, entry["archive_path"], entry["sha256"], entry["size"])
                output(runtime, "notices/" + package["name"] + "/" + entry["archive_path"], data)
        package_sources = {}
        for role, name, pin, data in (
            ("source", package["source_filename"], package["source_sha256"], source),
            ("recipe", package["recipe_filename"], package["recipe_sha256"], recipe),
            ("signature", package["filename"] + ".sig", package["signature_sha256"], sig),
        ):
            relative = "sources/" + portable_source_basename(name)
            existing = runtime / relative
            if existing.exists():
                if existing.is_symlink() or existing.read_bytes() != data:
                    raise ValueError("Conflicting shared source input")
            else:
                output(runtime, relative, data)
            package_sources[role] = {"original_filename": name, "path": relative, "sha256": pin}
        sources[package["name"]] = package_sources
    # Keep the runtime's byte-for-byte pinned manifest (and its upstream URLs)
    # intact. The sidecar describes only the filenames shipped under sources/.
    output(runtime, "MOSH-DEPENDENCIES.json", manifest_bytes)
    provenance = {"schema_version": 1, "packages": sources}
    output(runtime, "MOSH-SOURCES.json", (json.dumps(provenance, indent=2, sort_keys=True) + "\n").encode("utf-8"))

if __name__ == "__main__":
    import sys
    stage(*(Path(arg) for arg in sys.argv[1:]))
