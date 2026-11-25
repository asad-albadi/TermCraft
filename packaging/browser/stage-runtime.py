#!/usr/bin/env python3
"""Offline, pinned Chromium resource staging. No host browser discovery or downloads."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import os
import shutil
import subprocess
import tarfile
import tempfile


def digest(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError("Expected regular browser input")
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def checked(path, expected):
    if digest(path) != expected:
        raise ValueError("Pinned browser input checksum mismatch")


def stage(manifest_path, destination, archive, signature, notices=None):
    info = json.loads(manifest_path.read_text())
    checked(archive, info['archive_sha256'])
    checked(signature, info['archive_signature_sha256'])
    if notices is None:
        raise ValueError('Pinned third-party notices are required')
    checked(notices, info['notices_sha256'])
    final = destination / info['root']
    if os.path.lexists(final):
        raise FileExistsError('Browser payload already exists')
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.chromium-', dir=destination) as temp:
        root = Path(temp) / 'payload'
        root.mkdir()
        archives = [dict(id='chromium', path=archive), *[
            dict(id=dep['id'], path=archive.parent / dep['archive_name']) for dep in info.get('dependencies', [])]]
        for dep in info.get('dependencies', []):
            checked(archive.parent / dep['archive_name'], dep['archive_sha256'])
            checked(archive.parent / (dep['archive_name'] + '.sig'), dep['archive_signature_sha256'])
        for source in archives:
            expected = {item['archive_path']: item for item in info['files']
                        if 'archive_path' in item and item.get('archive', 'chromium') == source['id']}
            seen = set()
            with tempfile.TemporaryFile() as expanded:
                subprocess.run(['zstd', '-d', '-c', str(source['path'].resolve(strict=True))], stdout=expanded, check=True)
                expanded.seek(0)
                with tarfile.open(fileobj=expanded) as package:
                    for member in package:
                        if member.name not in expected:
                            continue
                        item = expected[member.name]
                        relative = PurePosixPath(item['path'])
                        if relative.is_absolute() or any(part in ('..', '.') for part in relative.parts):
                            raise ValueError('Unsafe pinned browser path')
                        if member.name in seen or not member.isfile() or member.size != item['size']:
                            raise ValueError('Invalid browser archive member')
                        seen.add(member.name)
                        output = root / str(relative)
                        output.parent.mkdir(parents=True, exist_ok=True)
                        with package.extractfile(member) as src, output.open('xb') as dst:
                            shutil.copyfileobj(src, dst)
                        checked(output, item['sha256'])
                        output.chmod(0o755 if item['executable'] else 0o644)
            if seen != set(expected):
                raise ValueError('Incomplete browser archive')
        for item in info['files']:
            if 'input_path' in item:
                src = archive.parent / item['input_path']
                checked(src, item['sha256'])
                shutil.copyfile(src, root / item['path'])
        shutil.copyfile(notices, root / 'THIRD-PARTY-NOTICES.html')
        shutil.copyfile(manifest_path, root / 'BROWSER-MANIFEST.json')
        final.parent.mkdir(parents=True, exist_ok=True)
        os.rename(root, final)
    return info


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('inputs', type=Path)
    args = parser.parse_args()
    info = json.loads(args.manifest.read_text())
    stage(args.manifest, args.destination, args.inputs / info['archive_name'],
          args.inputs / (info['archive_name'] + '.sig'), args.inputs / 'THIRD-PARTY-NOTICES.html')
