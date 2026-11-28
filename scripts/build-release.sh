#!/usr/bin/env bash
set -euo pipefail
termcraft_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
termcraft_network=()
if [[ "${TERMCRAFT_OFFLINE:-0}" == 1 ]]; then termcraft_network=(--offline); fi
cargo build --release --locked "${termcraft_network[@]}" --manifest-path "$termcraft_root/engines/rdp/Cargo.toml" --target-dir "$termcraft_root/engines/rdp/target"
cargo build --release --locked "${termcraft_network[@]}" --manifest-path "$termcraft_root/Cargo.toml" --target-dir "$termcraft_root/target"
termcraft_suffix=""
if [[ -f "$termcraft_root/engines/rdp/target/release/termcraft-rdp-engine.exe" ]]; then termcraft_suffix=".exe"; fi
python3 - "$termcraft_root" "$termcraft_suffix" <<'PYTHON'
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import tomllib

root, suffix = Path(sys.argv[1]), sys.argv[2]
bindir = root / "target/release"
helper = f"termcraft-rdp-engine{suffix}"
app = f"harbor-workspace{suffix}"
helper_source = root / "engines/rdp/target/release" / helper
manifest = bindir / "PACKAGE-MANIFEST.json"
provenance = "scripts/build-release.sh local source build"

if manifest.is_symlink() or (bindir / helper).is_symlink():
    raise SystemExit("Local release helper or manifest is a symbolic link.")
if manifest.exists():
    previous = json.loads(manifest.read_text(encoding="utf-8"))
    if (previous.get("profile"), previous.get("build", {}).get("provenance")) != (
        "local-build", provenance
    ):
        raise SystemExit("A different package owns the existing release manifest.")

versions = [
    tomllib.loads((root / p).read_text(encoding="utf-8"))["package"]["version"]
    for p in ("Cargo.toml", "engines/rdp/Cargo.toml")
]
architecture = platform.machine().lower()
architecture = {"amd64": "x86_64", "arm64": "aarch64"}.get(architecture, architecture)
system = "windows" if suffix else {"Linux": "linux", "Darwin": "macos"}.get(platform.system())
if not system or architecture not in {"aarch64", "x86_64"}:
    raise SystemExit("Unsupported local release platform or architecture.")

with tempfile.TemporaryDirectory(prefix=".termcraft-local-build-", dir=bindir) as temp:
    temporary = Path(temp)
    helper_image = temporary / helper
    shutil.copyfile(helper_source, helper_image)
    helper_image.chmod(0o755)
    files = {}
    for name, executable in [(app, bindir / app), (helper, helper_image)]:
        with executable.open("rb") as binary:
            files[name] = hashlib.file_digest(binary, "sha256").hexdigest()
    declaration = {
        "schema_version": 1, "application": "TermCraft",
        "profile": "local-build", "signing": "unsigned",
        "platform": system, "architecture": architecture,
        "version": versions[0], "rdp_helper_version": versions[1],
        "rdp_helper_wire_version": 2,
        "build": {"provenance": provenance}, "files": files,
    }
    declaration_file = temporary / manifest.name
    declaration_file.write_text(json.dumps(declaration, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(helper_image, bindir / helper)
    os.replace(declaration_file, manifest)
PYTHON
