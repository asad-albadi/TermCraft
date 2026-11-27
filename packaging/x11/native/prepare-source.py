#!/usr/bin/env python3
"""Prepare pinned source offline. Never executes an installer or native payload."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT=Path(__file__).resolve().parent
SOURCES={
 'windows':('a73b30db3eb6b60efd181a1cd7fa39bc6df8f0d4c973d6c8416a856b0bf55b28','vcxsrv-private-transport.patch'),
 'macos':('36db027489d6370ec5b1b6d07c8155c547193c2a6914fd617eb4311fb029257f','xquartz-private-entry.patch'),
}
def checksum(path):
    with Path(path).open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()
def prepare(platform, archive, destination):
    destination=Path(destination);archive=Path(archive)
    if destination.exists() or destination.is_symlink():raise FileExistsError('Source destination already exists')
    expected,patch=SOURCES[platform]
    patch_path = ROOT / patch
    patch_data = patch_path.read_bytes()
    patch_digest = hashlib.sha256(patch_data).hexdigest()
    contract = json.loads((ROOT / 'source-contract.json').read_text())
    if patch_digest != contract[platform]['patch_sha256']:
        raise ValueError('Pinned native patch checksum mismatch')
    with archive.open('rb') as source:
        data=source.read(128*1024*1024+1)
    if len(data)>128*1024*1024 or hashlib.sha256(data).hexdigest()!=expected:
        raise ValueError('Pinned native source checksum mismatch')
    with tempfile.TemporaryDirectory(prefix='termcraft-x11-source-') as work:
        tree=Path(work)/'tree';tree.mkdir()
        with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as package:
            # tarfile's data filter rejects traversal and escaping links.
            package.extractall(tree,filter='data')
        entries=list(tree.iterdir())
        if len(entries)!=1 or not entries[0].is_dir():raise ValueError('Unexpected pinned native source root')
        source=entries[0]
        subprocess.run(['patch','--batch','--forward','-p1'],cwd=source,input=patch_data,check=True)
        (source/'CORRESPONDING-SOURCE.tar.gz').write_bytes(data)
        (source/patch).write_bytes(patch_data)
        manifest={'schema':1,'platform':platform,'status':'unqualified private source candidate; never a runnable release',
                  'source_archive_sha256':expected,'patch_sha256':patch_digest,
                  'files':{str(p.relative_to(source)):checksum(p) for p in sorted(source.rglob('*')) if p.is_file() and not p.is_symlink()}}
        (source/'PREPARED-SOURCE.json').write_text(json.dumps(manifest,indent=2)+'\n')
        shutil.copytree(source,destination,symlinks=True)
    return destination
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform',choices=SOURCES,required=True)
    parser.add_argument('--source-archive',type=Path,required=True)
    parser.add_argument('--destination',type=Path,required=True)
    args=parser.parse_args();prepare(args.platform,args.source_archive,args.destination)
