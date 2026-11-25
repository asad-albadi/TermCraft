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
import subprocess,os,json,time,csv
from pathlib import Path
rows=[]
for version,port in [(2,25322),(3,25323)]:
 binary=f'/opt/termcraft/runtime/linux-x86_64/iperf{version}'
 env={'PATH':'','HOME':'/tmp/home','LANG':'C'}
 server=subprocess.Popen([binary,'-s','-p',str(port),*(['-y','C'] if version==2 else ['--one-off','--json'])],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
 try:
  time.sleep(.5)
  assert server.poll() is None
  client=subprocess.run([binary,'-c','127.0.0.1','-p',str(port),'-n','262144',*(['-y','C'] if version==2 else ['--json'])],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env,timeout=12)
  Path(f'/evidence/iperf{version}-client.log').write_bytes(client.stdout)
  assert client.returncode==0
  if version==3:
   server_output=server.communicate(timeout=5)[0]
  else:
   time.sleep(.5);server.terminate();server_output=server.communicate(timeout=3)[0]
  Path(f'/evidence/iperf{version}-server.log').write_bytes(server_output)
  if version==2:
   def byte_report(raw):
    records=list(csv.reader(raw.decode().splitlines())); assert len(records)==1,records
    fields=records[0];assert len(fields)==9,fields
    assert fields[1]=='127.0.0.1' and fields[3]=='127.0.0.1',fields
    assert float(fields[6].split('-')[1])>=0
    return int(fields[7])
   sent,received=byte_report(client.stdout),byte_report(server_output)
  else:
   c=json.loads(client.stdout);s=json.loads(server_output)
   assert 'error' not in c and 'error' not in s
   sent=c['end']['sum_sent']['bytes'];received=s['end']['sum_received']['bytes']
   assert c['end']['sum_received']['bytes']==received
  assert sent==received==262144,(sent,received)
  rows.append({'version':version,'client_exit':client.returncode,'requested_bytes':262144,'actual_sender_bytes':sent,'actual_receiver_bytes':received,'client_report':f'iperf{version}-client.log','server_report':f'iperf{version}-server.log'})
 finally:
  if server.poll() is None:
   server.terminate()
   try:server.communicate(timeout=3)
   except subprocess.TimeoutExpired:server.kill();server.communicate()
Path('/evidence/iperf-results.json').write_text(json.dumps(rows,indent=2)+'\n'); print(json.dumps(rows))
PY
