"""Demonstrate the unmodified historical gate accepting falsified disposable copies."""
from pathlib import Path
import tempfile,shutil,subprocess,json
root=Path.cwd();historical=root/'docs/parity-evidence/V08';out=historical/'clean-FIX1'
mutations=[('empty Iperf list','clean-iperf-results.json',lambda p:p.write_text('[]')),('empty Unix list','clean-unix-results.json',lambda p:p.write_text('[]')),('wrong Unix version','clean-unix-results.json',lambda p:p.write_text(json.dumps([{'version':'9.9.9'}]))),('garbage browser version','clean-runtime-browser-version.log',lambda p:p.write_text('BROKEN VERSION PROBE')),('missing minimal browser record','clean-bundled-browser.json',lambda p:p.unlink()),('unsupported minimal browser exit','clean-bundled-browser.json',lambda p:p.write_text(json.dumps(dict(json.loads(p.read_text()),exit_code=99)))),('unsupported omitted inventory exit','runtime-image-inventory.json',lambda p:p.write_text(json.dumps(dict(json.loads(p.read_text()),exit_code=99)))),('truncated Iperf receiver','clean-iperf2-server.log',lambda p:p.write_text('[1] 0-1 sec 1 Bytes 8 bits/sec'))]
results=[]
with tempfile.TemporaryDirectory(prefix='v08-fix1-red-matrix-') as temp:
 app=Path(temp)/'app';p=app/'docs/parity-evidence/V08';p.mkdir(parents=True);shutil.copytree(historical/'clean-run',p/'clean-run');shutil.copyfile(historical/'clean-analyze.py',p/'clean-analyze.py')
 for n in ['target','dist','scripts']:(app/n).symlink_to(root/n,target_is_directory=True)
 for label,name,mutate in mutations:
  path=p/'clean-run'/name;original=path.read_bytes()
  try:
   mutate(path);r=subprocess.run(['python',str(p/'clean-analyze.py')],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
   results.append({'case':label,'old_exit':r.returncode,'incorrectly_accepted':r.returncode==0});assert r.returncode==0,r.stdout
  finally:path.write_bytes(original)
(out/'RED-old-gate-matrix.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results,indent=2))
