#!/usr/bin/env python3
"""Independent read-only checks and separately mounted bounded GUI replay."""
import hashlib,json,os,posixpath,shutil,subprocess,tarfile,time
from pathlib import Path
out=Path(__file__).resolve().parent;base=out.parents[3]; replay=out/'review-replay'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def sealed():
 result={}
 for name in ['SHA256SUMS.json','SHA256SUMS-final.json']:
  entries=json.loads((out/name).read_text());bad=[n for n,h in entries.items() if sha(out/n)!=h];assert not bad,bad
  result[name]={'entries':len(entries),'mismatches':bad,'sha256':sha(out/name)}
 return result
result={'seals_before':sealed()}
provenance=json.loads((out/'export-provenance.json').read_text()); scratch=base/'target/validation/v08-clean-namespace';root=(scratch/'rootfs').resolve()
assert sha(scratch/'owned-rootfs.tar')==provenance['export_sha256']
files=links=0
with tarfile.open(scratch/'owned-rootfs.tar') as tf:
 members=tf.getmembers(); names={posixpath.normpath(m.name) for m in members}; linknames={posixpath.normpath(m.name) for m in members if m.issym() or m.islnk()}
 for m in members:
  p=Path(m.name);dest=root/p
  assert not p.is_absolute() and '..' not in p.parts
  assert not any(str(a) in linknames for a in p.parents if str(a)!='.')
  assert dest.resolve().is_relative_to(root)
  if m.isfile():
   assert hashlib.file_digest(tf.extractfile(m),'sha256').hexdigest()==sha(dest),m.name
   assert not dest.stat().st_mode&0o6000
   files+=1
  elif m.issym():
   expected=posixpath.relpath(m.linkname.lstrip('/'),posixpath.dirname(m.name) or '.') if m.linkname.startswith('/') else m.linkname
   assert dest.is_symlink() and os.readlink(dest)==expected,m.name
   links+=1
  elif m.islnk():
   assert os.path.samefile(dest,root/posixpath.normpath(m.linkname.lstrip('/'))),m.name
   links+=1
 extras=[]
 for p in root.rglob('*'):
  assert p.resolve().is_relative_to(root),p
  if p.relative_to(root).as_posix() not in names:extras.append(p.relative_to(root).as_posix())
 assert sorted(extras)==['evidence','opt/termcraft'],extras
 assert all(not list((root/n).iterdir()) for n in extras)
result['rootfs']={'regular_files':files,'contained_links':links,'export_sha256':provenance['export_sha256'],'extra_empty_dirs':extras}
image=json.loads(subprocess.check_output(['docker','image','inspect',provenance['image']],text=True))[0];assert image['Id']==provenance['image']
result['image_id']=image['Id']; inspect=subprocess.run(['docker','inspect',provenance['owned_container']],capture_output=True,text=True);assert inspect.returncode!=0 and 'no such' in inspect.stderr.lower()
result['export_container_absent']=True
integrity=json.loads((base/'docs/parity-evidence/parallel-integration-games-v03/release-integrity.json').read_text());pkg=Path(integrity['package_root']);manifest=json.loads((pkg/'PACKAGE-MANIFEST.json').read_text())
assert sha(Path(integrity['archive']))==integrity['archive_sha256']
assert manifest['build']['snapshot_sha256']==integrity['source_sha256']
assert all(sha(pkg/n)==h for n,h in manifest['files'].items())
result['package']={'files':len(manifest['files']),'source_sha256':integrity['source_sha256'],'archive_sha256':integrity['archive_sha256']}
def browsercheck(path):
 rows=json.loads(path.read_text());browsers=[r for r in rows if r.get('exe','').endswith('/chromium')];parent=next(r for r in browsers if '--type=' not in r['cmdline']);renderers=[r for r in browsers if '--type=renderer' in r['cmdline']]
 assert browsers and renderers
 for r in browsers:
  assert r['exe_sha256']==manifest['files']['browser/linux-x86_64/chromium']
  assert '--no-sandbox' not in r['cmdline'] and '--disable-setuid-sandbox' not in r['cmdline']
 for r in renderers:
  status=dict(line.split(':',1) for line in r['status'].splitlines() if ':' in line)
  assert status['Seccomp'].strip()=='2' and status['NoNewPrivs'].strip()=='1'
  assert set(status['Uid'].split())=={'1000'} and set(status['Gid'].split())=={'1000'}
  assert all(r['namespaces'][k]!=parent['namespaces'][k] for k in ['user','pid'])
 return {'executables_hash_matched':len(browsers),'protected_nested_renderers':len(renderers),'browser_sha256':parent['exe_sha256']}
result['sealed_native_browser']=browsercheck(out/'gui3-browser-processes.json')
assert (out/'gui3-pty-marker.txt').read_bytes()==b'V08_NAMESPACE_NATIVE_PTY_OK\n'
assert 'GET /receipt?value=V08_NATIVE_INPUT_OK HTTP/1.1' in (out/'gui3-http.log').read_text()
assert 'V08_SANDBOX_OFFLINE_OK' in (out/'headless-dom.txt').read_text()
assert (out/'runtime-packages.txt').read_bytes()==(base/'docs/parity-evidence/V08/clean-run/clean-runtime-image-packages.txt').read_bytes()
result['sealed_markers_and_runtime_inventory']=True
replay.mkdir(exist_ok=False)
for n in ['gui3.sh','inspect-gui3.py','browser-fixture.json']:shutil.copyfile(out/n,replay/n)
record=json.loads((out/'gui3-run.json').read_text());cmd=record['command'];i=cmd.index('--bind');assert cmd[i+1]==str(out) and cmd[i+2]=='/evidence';cmd[i+1]=str(replay)
(replay/'exact-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
t=time.monotonic();p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=90,start_new_session=True);(replay/'replay.log').write_bytes(p.stdout)
result['replay']={'command_file':'review-replay/exact-command.json','exit_code':p.returncode,'seconds':round(time.monotonic()-t,2)}
assert p.returncode==0,p.stdout
result['replay']['browser']=browsercheck(replay/'gui3-browser-processes.json')
assert (replay/'gui3-pty-marker.txt').read_bytes()==b'V08_NAMESPACE_NATIVE_PTY_OK\n'
assert 'GET /receipt?value=V08_NATIVE_INPUT_OK HTTP/1.1' in (replay/'gui3-http.log').read_text()
result['seals_after']=sealed()
# Match exact owned root/evidence paths, excluding this review process.
owned=[]
for entry in Path('/proc').iterdir():
 if not entry.name.isdigit():continue
 try:
  args=(entry/'cmdline').read_bytes().split(b'\0')
  if args and args[0].split(b'/')[-1]==b'bwrap' and str(root).encode() in args:owned.append(int(entry.name))
 except (OSError,ProcessLookupError):pass
assert not owned,owned
result['owned_bwrap_remaining']=owned
(out/'review-checks.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
