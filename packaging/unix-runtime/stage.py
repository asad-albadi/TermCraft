#!/usr/bin/env python3
"""Assemble pinned, unsigned TermCraft Unix archives from official inputs."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent


def verified(url, digest, cache):
    path = cache / digest
    if not path.exists():
        with urllib.request.urlopen(url, timeout=30) as response:
            payload = response.read(16 * 1024 * 1024 + 1)
        if hashlib.sha256(payload).hexdigest() != digest:
            raise ValueError(f"Upstream digest mismatch for {url}")
        path.write_bytes(payload)
    payload = path.read_bytes()
    if hashlib.sha256(payload).hexdigest() != digest:
        raise ValueError(f"Invalid cached artifact for {url}")
    return payload


def assemble(release, cache):
    binary = verified(release["binary_url"], release["binary_sha256"], cache)
    source = verified(release["source_url"], release["source_sha256"], cache)
    with tarfile.open(fileobj=io.BytesIO(source), mode="r:*") as archive:
        license_file = archive.extractfile(release["license_path"]).read()
    if hashlib.sha256(license_file).hexdigest() != release["license_sha256"]:
        raise ValueError(f"License digest mismatch for {release['version']}")
    result = io.BytesIO()
    with gzip.GzipFile(fileobj=result, mode="wb", mtime=0, compresslevel=6) as gzip_file:
        with tarfile.open(fileobj=gzip_file, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for path, contents, mode in [
                *((f"bin/{path}", binary, 0o755) for path in sorted(release["commands"].values())),
                ("notices/LICENSE", license_file, 0o644),
            ]:
                entry = tarfile.TarInfo(path)
                entry.size = len(contents)
                entry.mode = mode
                archive.addfile(entry, io.BytesIO(contents))
    return result.getvalue()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "windows"))
    parser.add_argument("--version")
    parser.add_argument("--show-digests", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    args.cache.mkdir(parents=True, exist_ok=True)
    index = json.loads((HERE / "manifest.json").read_text())
    releases = [
        release for release in index["releases"]
        if (args.platform is None or release["platform"] == args.platform)
        and (args.version is None or release["version"] == args.version)
    ]
    if not releases:
        parser.error("No pinned package matches the selected platform and version")
    for release in releases:
        archive = assemble(release, args.cache)
        digest = hashlib.sha256(archive).hexdigest()
        filename = release["file_name"]
        print(f"{digest}  {filename}")
        if args.show_digests:
            continue
        if digest != release["sha256"]:
            raise ValueError(f"Generated archive digest mismatch for {filename}")
        with tempfile.NamedTemporaryFile(dir=args.output, delete=False) as staged:
            staged.write(archive)
        Path(staged.name).replace(args.output / filename)
        source_name = release["source_url"].rsplit("/", 1)[-1]
        shutil.copyfile(args.cache / release["source_sha256"], args.output / source_name)
        if "config_url" in release:
            config = verified(release["config_url"], release["config_sha256"], args.cache)
            (args.output / f'busybox-{release["version"]}.config').write_bytes(config)


if __name__ == "__main__":
    main()
