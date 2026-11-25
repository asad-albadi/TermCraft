#!/usr/bin/env python3
import hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parents[4];out=Path(__file__).resolve().parent
integrity=json.loads((base/'docs/parity-evidence/parallel-integration-games-v03/release-integrity.json').read_text());pkg=Path(integrity['package_root']);archive=Path(integrity['archive'])
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
manifest=json.loads((pkg/'PACKAGE-MANIFEST.json').read_text());bad=[]
for name,h in manifest['files'].items():
 if sha(pkg/name)!=h:bad.append(name)
if bad:raise RuntimeError(bad)
if sha(archive)!=integrity['archive_sha256']:raise RuntimeError('archive mismatch')
if manifest['build']['snapshot_sha256']!=integrity['source_sha256']:raise RuntimeError('source mismatch')
rows=json.loads((out/'gui3-browser-processes.json').read_text());browsers=[r for r in rows if r.get('exe','').endswith('/chromium')];parent=next(r for r in browsers if '--type=' not in r['cmdline']);renderers=[r for r in browsers if '--type=renderer' in r['cmdline']]
for r in browsers:
 if r['exe_sha256']!=manifest['files']['browser/linux-x86_64/chromium']:raise RuntimeError('executed browser bytes mismatch')
 if '--no-sandbox' in r['cmdline'] or '--disable-setuid-sandbox' in r['cmdline']:raise RuntimeError('sandbox bypass')
for r in renderers:
 if '\nSeccomp:\t2\n' not in r['status'] or '\nNoNewPrivs:\t1\n' not in r['status']:raise RuntimeError('renderer protection missing')
 if r['namespaces']['user']==parent['namespaces']['user'] or r['namespaces']['pid']==parent['namespaces']['pid']:raise RuntimeError('no nested sandbox')
if not renderers:raise RuntimeError('no renderer')
result={'archive_sha256':integrity['archive_sha256'],'source_sha256':integrity['source_sha256'],'manifest_files_verified':len(manifest['files']),'manifest_mismatches':bad,'native_browser_executed_sha256':parent['exe_sha256'],'native_browser_hash_verified_processes':len(browsers),'native_sandboxed_renderers':len(renderers),'browser_parent_namespace':parent['namespaces'],'native_renderers':[{'pid':r['pid'],'namespaces':r['namespaces']} for r in renderers],'runtime_inventory_matches_prior':(out/'runtime-packages.txt').read_bytes()==(base/'docs/parity-evidence/V08/clean-run/clean-runtime-image-packages.txt').read_bytes()}
(out/'verified-inputs.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
