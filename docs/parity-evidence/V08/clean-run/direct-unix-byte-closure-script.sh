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
import os,pty,select,subprocess,tarfile,json,hashlib
from pathlib import Path
rows=[]
for version in ['1.31.0','1.35.0']:
 archive=Path(f'/opt/termcraft/unix-packages/busybox-{version}-linux-x86_64.tar.gz')
 target=Path('/tmp/unix-'+version);target.mkdir()
 with tarfile.open(archive) as t:
  entries=[x for x in t.getmembers() if x.isfile() and x.name.endswith('/busybox')]
  assert len(entries)==1,[x.name for x in t.getmembers()]
  binary=target/'busybox';binary.write_bytes(t.extractfile(entries[0]).read());binary.chmod(0o700)
 master,slave=pty.openpty()
 command=f'''test -t 0 && test -t 1 && printf 'UNIX_REAL_PTY_{version}\\n' && printf 'alphabeta' | {binary} tr a-z A-Z'''
 p=subprocess.Popen([str(binary),'ash','-c',command],stdin=slave,stdout=slave,stderr=slave,env={'PATH':'','HOME':'/tmp/home','LANG':'C'});os.close(slave);data=b''
 try:
  p.wait(timeout=5)
  while select.select([master],[],[],0.1)[0]:
   try: chunk=os.read(master,65536)
   except OSError:break
   if not chunk:break
   data+=chunk
  assert p.returncode==0,data
  assert f'UNIX_REAL_PTY_{version}'.encode() in data and b'ALPHABETA' in data,data
 finally:
  if p.poll() is None:p.kill();p.wait()
  os.close(master)
 Path(f'/evidence/clean-unix-{version}.log').write_bytes(data)
 rows.append({'version':version,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'exit':p.returncode,'provider':'extracted archive direct PTY byte closure, not PackageManager integration'})
Path('/evidence/clean-unix-results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
PY
