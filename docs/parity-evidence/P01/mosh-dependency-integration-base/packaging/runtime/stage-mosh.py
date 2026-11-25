#!/usr/bin/env python3
"""Stage the pinned Arch Linux Mosh client without installing system packages."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile


def checked(payload, expected, label):
    digest = hashlib.sha256(payload).hexdigest()
    if digest != expected:
        raise ValueError(f"{label} checksum mismatch")
    return payload


def input_bytes(path, digest, label):
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"{label} must be a regular file")
    if path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError(f"{label} exceeds the input size limit")
    return checked(path.read_bytes(), digest, label)


def archive_member(archive, name):
    member = archive.getmember(name)
    if not member.isfile() or not 0 < member.size < 8 * 1024 * 1024:
        raise ValueError(f"Expected a bounded regular member at {name}")
    with archive.extractfile(member) as extracted:
        return extracted.read()


def stage(manifest_path, destination, binary_archive, source_archive, recipe):
    manifest = json.loads(manifest_path.read_text())
    wrapper, client = manifest["artifacts"]
    inputs = manifest["mosh_inputs"]
    package_bytes = input_bytes(binary_archive, client["archive_sha256"], "Pinned Mosh binary archive")
    source_bytes = input_bytes(source_archive, client["source_sha256"], "Pinned Mosh source archive")
    recipe_bytes = input_bytes(recipe, client["arch_recipe_sha256"], "Pinned Mosh Arch recipe")
    # Read entries from the authenticated input. Never extract arbitrary archive paths.
    binary_tar = subprocess.run(
        ["zstd", "-d", "-c"], input=package_bytes, stdout=subprocess.PIPE, check=True,
    ).stdout
    with tarfile.open(fileobj=io.BytesIO(binary_tar)) as package:
        raw_wrapper = checked(
            archive_member(package, "usr/bin/mosh"),
            wrapper["original_sha256"],
            "Upstream Mosh wrapper",
        )
        wrapper_bytes = checked(
            raw_wrapper.replace(b"#!/usr/bin/env perl\n", b"#!/usr/bin/perl\n", 1),
            wrapper["sha256"],
            "TermCraft Mosh wrapper",
        )
        client_bytes = checked(
            archive_member(package, "usr/bin/mosh-client"),
            client["sha256"],
            "Mosh client",
        )
        inventory = {
            name: checked(archive_member(package, name), digest, name)
            for name, digest in inputs["archive_inventory"].items()
        }
    with tarfile.open(fileobj=io.BytesIO(source_bytes)) as source:
        license_bytes = checked(
            archive_member(source, "mosh-mosh-1.4.0/COPYING"),
            inputs["source_license_sha256"],
            "Mosh COPYING",
        )
    runtime = destination / "runtime/linux-x86_64"
    runtime.mkdir(parents=True, exist_ok=False)
    for name, data in {"mosh": wrapper_bytes, "mosh-client": client_bytes}.items():
        executable = runtime / name
        executable.write_bytes(data)
        executable.chmod(0o755)
    for name, data in {
        **inventory,
        "COPYING": license_bytes,
        "mosh-1.4.0.tar.gz": source_bytes,
        "PKGBUILD": recipe_bytes,
    }.items():
        (runtime / name.lstrip(".")).write_bytes(data)
    (runtime / "termcraft-mosh.patch").write_text(
        "--- a/usr/bin/mosh\n+++ b/usr/bin/mosh\n@@ -1 +1 @@\n"
        "-#!/usr/bin/env perl\n+#!/usr/bin/perl\n"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ("manifest", "destination", "binary_archive", "source_archive", "recipe"):
        parser.add_argument(argument, type=Path)
    stage(**{
        "manifest_path" if name == "manifest" else name: path
        for name, path in vars(parser.parse_args()).items()
    })
