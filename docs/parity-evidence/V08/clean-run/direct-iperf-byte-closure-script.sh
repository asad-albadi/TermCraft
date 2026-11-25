set -euo pipefail
mkdir -p /tmp/home /tmp/wayland
chmod 700 /tmp/wayland
export DISPLAY=:77
Xvfb :77 -screen 0 1360x880x24 -nolisten tcp > /tmp/xvfb.log 2>&1 & xp=$!
ap=''
cleanup() { if [[ -n "$ap" ]]; then kill "$ap" 2>/dev/null || true; wait "$ap" 2>/dev/null || true; fi; kill "$xp" 2>/dev/null || true; wait "$xp" 2>/dev/null || true; }
trap cleanup EXIT
sleep 1
launch() { /opt/termcraft/harbor-workspace --data-dir "/evidence/$1" > "/evidence/$1-launch.log" 2>&1 & ap=$!; sleep 4; win=$(xdotool search --pid "$ap" | head -1); xdotool windowfocus --sync "$win"; }
stop() { kill "$ap"; wait "$ap" || true; ap=''; }
click() { xdotool mousemove --sync "$1" "$2" click 1; sleep 0.5; }
shot() { import -window root "/evidence/$1.png"; }
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
