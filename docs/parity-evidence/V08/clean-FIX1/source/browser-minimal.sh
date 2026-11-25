MODE=minimal
python - <<'PY'
from pathlib import Path
import json
p=Path('/evidence/fixture-browser');p.mkdir(exist_ok=True)
(p/'workspace.json').write_text(json.dumps({'sessions':[{'id':'0123456789abcdef0123456789abcdef','name':'FIX1 bundled browser','protocol':'Browser','host':'http://127.0.0.1:25380','browser_options':{'embedded':True,'runtime':'BundledChromium'}}]}))
PY
python -m http.server 25380 --bind 127.0.0.1 --directory /tmp > "/evidence/browser-$MODE-http.log" 2>&1 & hp=$!
trap 'kill "$hp" 2>/dev/null || true; wait "$hp" 2>/dev/null || true; cleanup' EXIT
launch fixture-browser
/opt/termcraft/harbor-workspace --data-dir /evidence/fixture-browser --session 0123456789abcdef0123456789abcdef
sleep 6
shot "browser-$MODE"
export MODE
python - <<'PY'
from pathlib import Path
import subprocess,os,signal,json,ctypes
mode=os.environ['MODE'];env=dict(os.environ,LD_LIBRARY_PATH='/opt/termcraft/browser/linux-x86_64/libraries')
binary='/opt/termcraft/browser/linux-x86_64/chromium'
def run(args,label):
 p=subprocess.Popen([binary,*args],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env,start_new_session=True)
 timed_out=False
 try:raw=p.communicate(timeout=12)[0]
 except subprocess.TimeoutExpired:
  timed_out=True;os.killpg(p.pid,signal.SIGKILL);raw=p.communicate()[0]
 finally:
  try:os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError:pass
 Path(f'/evidence/browser-{mode}-{label}.log').write_bytes(raw)
 return {'exit_code':p.returncode,'timed_out':timed_out,'log':f'browser-{mode}-{label}.log'},raw.decode(errors='replace')
version,text=run(['--version'],'version');result={'mode':mode,'version':version}
assert not version['timed_out']
if mode=='minimal':
 assert version['exit_code']==127 and 'libopenh264.so.8' in text and 'cannot open shared object file' in text,text
 result.update(status='EXPECTED_DEPENDENCY_ERROR',qualification='NEGATIVE_ONLY')
else:
 assert version['exit_code']==0 and text.strip()=='Chromium 158.0.8127.88',text
 native,raw=run(['--headless','--disable-gpu','--dump-dom','http://127.0.0.1:25380'],'native')
 assert not native['timed_out'] and native['exit_code']==-signal.SIGABRT,(native,raw)
 assert 'SUID sandbox helper binary was found, but is not configured correctly' in raw
 assert 'mode 4755' in raw
 libc=ctypes.CDLL(None,use_errno=True);rc=libc.unshare(0x10000000);err=ctypes.get_errno()
 assert rc==-1 and err==1,(rc,err)
 result.update(status='SANDBOX_BLOCKED',qualification='UNAVAILABLE',native=native,user_namespace={'return_code':rc,'errno':err})
Path(f'/evidence/browser-{mode}-outcome.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
PY
