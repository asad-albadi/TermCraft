python - <<'PY'
import subprocess,os,json,time
from pathlib import Path
rows=[]
for version,port in [(2,25322),(3,25323)]:
 binary=f'/opt/termcraft/runtime/linux-x86_64/iperf{version}'
 env={'PATH':'','HOME':'/tmp/home','LANG':'C'}
 server=subprocess.Popen([binary,'-s','-p',str(port)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
 try:
  time.sleep(1)
  if server.poll() is not None: raise RuntimeError(server.communicate()[0].decode())
  maps=Path(f'/proc/{server.pid}/maps').read_text();Path(f'/evidence/clean-iperf{version}-maps.txt').write_text(maps)
  client=subprocess.run([binary,'-c','127.0.0.1','-p',str(port),'-n','262144'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env,timeout=12)
  Path(f'/evidence/clean-iperf{version}-client.log').write_bytes(client.stdout)
  assert client.returncode==0,client.stdout.decode()
  assert b'bits/sec' in client.stdout,client.stdout.decode()
  rows.append({'version':version,'client_exit':client.returncode,'bytes_requested':262144,'provider':'extracted packaged ELF; direct byte closure, not app managed API'})
 finally:
  server.terminate()
  try: output=server.communicate(timeout=3)[0]
  except subprocess.TimeoutExpired: server.kill();output=server.communicate()[0]
  Path(f'/evidence/clean-iperf{version}-server.log').write_bytes(output)
Path('/evidence/clean-iperf-results.json').write_text(json.dumps(rows,indent=2)+'\n'); print(json.dumps(rows))
PY
