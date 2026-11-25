#!/usr/bin/env python3
"""Stage qualified Iperf binaries and their corresponding source/license files.

Usage: stage-runtime.py MANIFEST PAYLOAD_ROOT DESTINATION
All paths in the committed manifest are relative to PAYLOAD_ROOT. Never run engines
from an unchecked directory or accept a new checksum from the payload itself.
"""
import hashlib
import json
from pathlib import Path
import shutil
import sys

def checked(path, digest=None, limit=16*1024*1024):
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= limit:
        raise ValueError(f"Expected bounded regular runtime input: {path.name}")
    data = path.read_bytes()
    if digest and hashlib.sha256(data).hexdigest() != digest:
        raise ValueError(f"Runtime checksum mismatch: {path.name}")
    return data

def stage(manifest, payload, destination):
    info = json.loads(checked(manifest))
    if info['schema_version'] != 1 or {a['tool'] for a in info['artifacts']} != {'iperf2','iperf3'}:
        raise ValueError('Expected qualified Iperf2 and Iperf3 declarations')
    for artifact in info['artifacts']:
        relative = Path(artifact['path'])
        if relative.is_absolute() or '..' in relative.parts or relative.parts != ('runtime','linux-x86_64',artifact['tool']):
            raise ValueError('Invalid Iperf runtime path')
        binary = checked(payload / relative, artifact['sha256'])
        source = checked((payload / relative).with_name(artifact['tool']+'-source.tar.gz'),artifact['source_sha256'])
        license = checked((payload / relative).with_name(artifact['tool']+'-LICENSE'),artifact['license_sha256'])
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        for path, data in [(target,binary),(target.with_name(artifact['tool']+'-source.tar.gz'),source),(target.with_name(artifact['tool']+'-LICENSE'),license)]:
            with path.open('xb') as file:
                file.write(data)
        target.chmod(0o755)
    shutil.copyfile(manifest, destination/'runtime/linux-x86_64/IPERF-MANIFEST.json')

if __name__ == '__main__':
    stage(*(Path(arg).resolve() for arg in sys.argv[1:]))
