#!/usr/bin/env python3
"""Offline, pinned Chromium resource staging. No host browser discovery or downloads."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import os
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import zipfile


def digest(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError("Expected regular browser input")
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def checked(path, expected):
    if digest(path) != expected:
        raise ValueError("Pinned browser input checksum mismatch")


def windows_member(name):
    parts = name.split('/')
    if any(not re.fullmatch(r'[A-Za-z0-9_. -]+', part) or part.endswith(('.', ' '))
           or re.fullmatch(r'(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?', part, re.I)
           for part in parts):
        raise ValueError('Unsafe Chromium ZIP path')
    return Path(*parts)


def stage_windows(manifest_path, destination, inputs):
    manifest = manifest_path.read_bytes()
    info = json.loads(manifest)
    if (info.get('schema_version'), info.get('platform'), info.get('architecture'),
        info.get('root'), info.get('executable')) != (
            1, 'windows', 'x86_64', 'browser/windows-x86_64', 'chrome.exe') or info.get('approved') is not True:
        raise ValueError('No qualified bundled Chromium payload for Windows x64')
    archive_name = windows_member(info['archive_name'])
    if len(archive_name.parts) != 1 or archive_name.suffix != '.zip':
        raise ValueError('Expected a pinned Chromium ZIP filename')
    archive = inputs / archive_name
    expected = {item['path']: item for item in info['files']}
    aliases = {name.lower() for name in expected}
    if (len(aliases) != len(info['files']) or 'browser-manifest.json' in aliases
        or not expected.get('chrome.exe', {}).get('executable')
        or 'THIRD-PARTY-NOTICES.html' not in expected):
        raise ValueError('Incomplete or ambiguous Chromium inventory')
    paths = {name: windows_member(name) for name in expected}
    final = destination / info['root']
    if os.path.lexists(final):
        raise FileExistsError('Browser payload already exists')
    if archive.is_symlink() or not archive.is_file():
        raise ValueError('Expected regular browser input')
    with archive.open('rb') as archive_file:
        if hashlib.file_digest(archive_file, 'sha256').hexdigest() != info['archive_sha256']:
            raise ValueError('Pinned browser input checksum mismatch')
        archive_file.seek(0)
        with zipfile.ZipFile(archive_file) as package:
            members = package.infolist()
            if len(members) != len(expected) or {m.filename for m in members} != expected.keys():
                raise ValueError('Unexpected Chromium ZIP member or incomplete archive')
            destination.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(prefix='.chromium-', dir=destination) as temp:
                root = Path(temp) / 'payload'
                root.mkdir()
                for member in members:
                    item = expected[member.filename]
                    mode = stat.S_IFMT(member.external_attr >> 16)
                    if (member.is_dir() or mode not in (0, stat.S_IFREG)
                        or member.file_size != item['size'] or not 0 <= item['size'] <= 1024**3):
                        raise ValueError('Invalid Chromium ZIP member')
                    output = root / paths[member.filename]
                    output.parent.mkdir(parents=True, exist_ok=True)
                    with package.open(member) as src, output.open('xb') as dst:
                        shutil.copyfileobj(src, dst)
                    checked(output, item['sha256'])
                (root / 'BROWSER-MANIFEST.json').write_bytes(manifest)
                final.parent.mkdir(parents=True, exist_ok=True)
                os.rename(root, final)
    return info


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
    if info['platform'] == 'windows':
        stage_windows(args.manifest, args.destination, args.inputs)
    else:
        stage(args.manifest, args.destination, args.inputs / info['archive_name'],
              args.inputs / (info['archive_name'] + '.sig'), args.inputs / 'THIRD-PARTY-NOTICES.html')
