import ast,ctypes,os,subprocess,sys,tempfile,time,types,json,signal
from pathlib import Path
ctypes.CDLL(None).prctl(36,1,0,0,0)
base=Path(sys.argv[1]);results=[]
with tempfile.TemporaryDirectory() as tmp:
 w=Path(tmp); launched=[]
 tree=ast.parse((base/'run_mosh_roaming.py').read_text()); f=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='run_case')
 def popen(*a,**k):
  p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],start_new_session=True);launched.append(p)
  def wait(timeout=None): raise subprocess.TimeoutExpired('injected-startup',timeout)
  p.wait=wait;return p
 class Sock:
  def close(self):pass
 ns=dict(work=w,OUT=w,child=w/"dummy",cmd=lambda *a:'[]',sock=lambda *a:Sock(),subprocess=types.SimpleNamespace(Popen=popen),sys=sys,os=os,time=time,re=__import__('re'),signal=signal,Path=Path)
 exec(compile(ast.Module(body=[f],type_ignores=[]),'exact-baseline-run_case','exec'),ns)
 try:ns['run_case']('fault')
 except BaseException as e:err=str(e)
 p=launched[0];alive=Path('/proc',str(p.pid)).exists();results.append({'probe':'launcher-wait-timeout-before-owner','expected':'no live descendants','actual_launcher_alive':alive,'exception':err,'red_reproduced':alive})
 os.kill(p.pid,signal.SIGKILL);os.waitpid(p.pid,0)
 pidfile=w/'descendant'
 program='import subprocess,sys,time; p=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);open(sys.argv[1],"w").write(str(p.pid));time.sleep(60)'
 try:subprocess.run([sys.executable,'-c',program,str(pidfile)],timeout=.2,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 except subprocess.TimeoutExpired:pass
 pid=int(pidfile.read_text());alive=Path('/proc',str(pid)).exists();results.append({'probe':'outer-timeout-detached-descendant','expected':'no live descendants','actual_descendant_alive':alive,'red_reproduced':alive});os.kill(pid,signal.SIGKILL);os.waitpid(pid,0)
assert all(r['red_reproduced'] for r in results)
print(json.dumps({'status':'RED-confirmed','results':results,'probe_processes_reaped':True},indent=2))
