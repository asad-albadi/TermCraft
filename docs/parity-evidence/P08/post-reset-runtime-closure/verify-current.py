from pathlib import Path
import json,hashlib,os,re
out=Path(__file__).resolve().parent;r=json.loads((out/'payload-equivalence.json').read_text());pkg=Path(r['package']);manifest=json.loads((pkg/'PACKAGE-MANIFEST.json').read_text());bad=[]
for name,expected in manifest['files'].items():
 p=pkg/name
 if not p.is_file() or p.is_symlink() or hashlib.file_digest(p.open('rb'),'sha256').hexdigest()!=expected:bad.append(name)
assert not bad,bad
assert manifest['build']['snapshot_sha256']=='22bbf26840638474044431d242b92efa076123e87f39849cf0c8d967b33ecb04'
assert r['app_sha256']=='c8d09ba5fd2c36d567c6885f117d6068e2a87185648db883853cde84d678f7e6'
rows=json.loads((out/'browser-current-browser-processes.json').read_text());renderers=[r for r in rows if '--type=renderer' in r.get('cmdline','')];parents=[r for r in rows if '--type=' not in r.get('cmdline','') and 'termcraft-browser-runtime-' in r.get('exe','')];assert parents
sha=hashlib.file_digest((pkg/'browser/linux-x86_64/chromium').open('rb'),'sha256').hexdigest()
for renderer in renderers:
 assert renderer['exe_sha256']==sha
 assert 'Seccomp:\t2' in renderer['status'] and 'NoNewPrivs:\t1' in renderer['status']
 assert renderer['namespaces']['user']!=parents[0]['namespaces']['user']
 assert renderer['namespaces']['pid']!=parents[0]['namespaces']['pid']
assert len(renderers)>=1
http=(out/'browser-current-http.log').read_text();assert 'GET /offline.html HTTP/1.1" 200' in http
assert '/receipt?value=V08_NATIVE_INPUT_OK ' in http
sizes=set(re.findall(r'/resize\?w=(\d+)&h=(\d+)',http));assert len(sizes)>=2
assert json.loads((out/'browser-current-result.json').read_text())['exit_code']==0
assert not json.loads((out/'browser-current-cleanup.json').read_text())['remaining']
assert (out/'browser-current-pty-marker.txt').read_bytes()==b'V08_NAMESPACE_NATIVE_PTY_OK\n'
res={'manifest_verified_files':len(manifest['files']),'source_sha256':manifest['build']['snapshot_sha256'],'app_sha256':r['app_sha256'],'browser_sha256':sha,'sandboxed_renderers':len(renderers),'browser_viewport_sizes':sorted(sizes),'browser_result':'PASS','browser_http_fixture_receipts':'404 paths recorded by minimal server; exact query proves input/resize; initial document200','payload_equivalent_files':len(r['new_inventory']),'payload_changes':r['changed']}
(out/'verification-browser.json').write_text(json.dumps(res,indent=2)+'\n');print(json.dumps(res,indent=2))
