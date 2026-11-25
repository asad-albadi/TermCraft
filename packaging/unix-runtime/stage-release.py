#!/usr/bin/env python3
"""Stage pinned Unix archives offline for a release; never execute upstream inputs."""
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import struct
import tarfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def regular(path):
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError(f'Expected a regular Unix payload input: {path}')
    return path.read_bytes()


def cached(cache, expected):
    if not re.fullmatch('[0-9a-f]{64}', expected):
        raise ValueError('Invalid Unix payload digest')
    data = regular(cache / expected)
    if digest(data) != expected:
        raise ValueError(f'Unix payload checksum mismatch: {expected}')
    return data


def native_format(data, target):
    """Inspect file headers only, including static ELF segments; no execution/ldd."""
    if target == 'linux' and len(data) >= 64 and data[:6] == b'\x7fELF\x02\x01':
        kind, machine = struct.unpack_from('<HH', data, 16)
        phoff = struct.unpack_from('<Q', data, 32)[0]
        size, count = struct.unpack_from('<HH', data, 54)
        if kind != 2 or machine != 62 or size < 56 or phoff + size * count > len(data):
            raise ValueError('Unix payload requires a static ELF x86_64 executable')
        for index in range(count):
            if struct.unpack_from('<I', data, phoff + size * index)[0] in (2, 3):
                raise ValueError('Unix payload requires static ELF without dynamic/interpreter segments')
        return 'ELF-x86_64-static'
    if target == 'windows' and len(data) >= 64 and data[:2] == b'MZ':
        offset = struct.unpack_from('<I', data, 60)[0]
        if offset + 26 <= len(data) and data[offset:offset+4] == b'PE\0\0':
            machine = struct.unpack_from('<H', data, offset+4)[0]
            flags = struct.unpack_from('<H', data, offset+22)[0]
            optional = struct.unpack_from('<H', data, offset+24)[0]
            if machine == 0x8664 and flags & 2 and not flags & 0x2000 and optional == 0x20b:
                return 'PE-x86_64'
    raise ValueError(f'Unix payload executable does not match {target} x86_64')


def stage(here, destination, cache, target, architecture):
    if (target, architecture) not in (('linux', 'x86_64'), ('windows', 'x86_64')):
        raise ValueError('Unix payload supports Linux and Windows x86_64 only')
    index = json.loads(regular(here / 'manifest.json'))
    releases = [r for r in index['releases'] if (r['platform'], r['architecture']) == (target, architecture)]
    if index['schema'] != 1 or not releases:
        raise ValueError('No pinned Unix payload for target')
    spec = importlib.util.spec_from_file_location('unix_assembler', here / 'stage.py')
    assembler = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(assembler)
    # Replace the generic downloader with a strict local reader, even if cache files
    # disappear after preflight. Release packaging has no network fallback.
    assembler.verified = lambda url, expected, selected_cache: cached(selected_cache, expected)
    payloads = []
    # Check every input before the assembler can use its network-enabled fallback.
    for release in releases:
        binary = cached(cache, release['binary_sha256'])
        source = cached(cache, release['source_sha256'])
        config = cached(cache, release['config_sha256']) if 'config_sha256' in release else None
        payloads.append((release, binary, source, config, native_format(binary, target)))
    output = destination / 'unix-packages'
    output.mkdir(parents=True, exist_ok=False)
    result = []
    def write(relative, data):
        path = output / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as file:
            file.write(data)
        return path.relative_to(destination).as_posix()
    notice_index = json.loads(regular(here / 'notices/manifest.json'))
    musl_paths = []
    musl_references = []
    if target == 'linux':
        for item in notice_index['musl_reference_notices']:
            for key, sha_key in (('notice_file','notice_sha256'), ('copyright_file','copyright_sha256')):
                name = item[key]
                if Path(name).name != name:
                    raise ValueError('Invalid musl notice filename')
                contents = regular(here / 'notices' / name)
                if digest(contents) != item[sha_key]:
                    raise ValueError('Unix musl notice checksum mismatch')
                musl_paths.append(write(Path('notices/musl') / name, contents))
            musl_references.append(item)
    for release, binary, source, config, file_format in payloads:
        archive = assembler.assemble(release, cache)
        if digest(archive) != release['sha256']:
            raise ValueError('Pinned Unix archive checksum mismatch')
        item = dict(release)
        item['archive_path'] = write(release['file_name'], archive)
        source_name = release['source_url'].rsplit('/', 1)[-1]
        item['source_path'] = write(source_name, source)
        item['native_format'] = file_format
        item['notice_paths'] = list(musl_paths)
        item['build_information'] = []
        for material in notice_index.get('upstream_materials', []):
            if (material['platform'], material['version']) != (target, release['version']):
                continue
            name = material['file']
            if Path(name).name != name:
                raise ValueError('Invalid Unix upstream material filename')
            contents = regular(here / 'notices' / name)
            if digest(contents) != material['sha256']:
                raise ValueError('Unix upstream material checksum mismatch')
            copied = dict(material)
            copied['path'] = write(Path('build-information') / name, contents)
            item['build_information'].append(copied)
        with tarfile.open(fileobj=io.BytesIO(source), mode='r:*') as upstream:
            for member in upstream:
                # Preserve all named upstream license/attribution files; all source-level
                # comments remain in the exact, hash-verified corresponding source archive.
                if member.isfile() and re.fullmatch(r'(LICENSE|COPYING|COPYRIGHT|NOTICE)([._-].*)?', Path(member.name).name, re.I):
                    relative = Path(member.name)
                    if relative.is_absolute() or '..' in relative.parts:
                        raise ValueError('Unsafe Unix source notice path')
                    contents = upstream.extractfile(member).read()
                    item['notice_paths'].append(write(Path('notices') / release['version'] / relative, contents))
        if config is not None:
            item['config_path'] = write(f"busybox-{release['version']}.config", config)
        item['linked_components'] = ([{
            'name': 'musl libc (published static upstream build)',
            'linked_version': None, 'reference_notices': musl_references,
            'notice_paths': musl_paths,
        }] if target == 'linux' else [{
            'name': 'Windows system DLL imports',
            'qualification': 'PE format verified; Windows runtime execution not performed by staging',
        }])
        item['provenance_limits'] = [
            'Pinned upstream binary and corresponding published source; not a reproduced source build.',
            'Upstream toolchain and exact build recipe are not verified.',
        ]
        if target == 'linux':
            item['provenance_limits'].append('Exact linked musl version is unidentified; reference musl notices are retained conservatively, including third-party source license comments.')
        if config is None:
            item['provenance_limits'].append('No standalone pinned build configuration; source-tree configurations and any declared published build logs are retained.')
        result.append(item)
    inventory = {'schema': 1, 'platform': target, 'architecture': architecture,
                 'archive_directory': 'unix-packages', 'releases': result,
                 'qualification': 'Offline hash and native-format verification; no payload execution by packaging.'}
    write('INVENTORY.json', (json.dumps(inventory, indent=2, sort_keys=True)+'\n').encode())
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    parser.add_argument('cache', type=Path)
    parser.add_argument('platform', choices=('linux', 'windows'))
    parser.add_argument('architecture', choices=('x86_64',))
    args = parser.parse_args()
    try:
        stage(Path(__file__).resolve().parent, args.destination, args.cache, args.platform, args.architecture)
    except (OSError, ValueError, tarfile.TarError) as error:
        parser.exit(1, f'Unix payload staging failed: {error}\n')

if __name__ == '__main__':
    main()
