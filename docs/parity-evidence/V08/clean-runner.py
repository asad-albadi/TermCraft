#!/usr/bin/env python3
"""Owned clean Linux GUI runner: read-only package, dedicated evidence, isolated display/network."""
import argparse,hashlib,json,subprocess,time,uuid,sys
from pathlib import Path
ap=argparse.ArgumentParser();ap.add_argument('name');ap.add_argument('script',type=Path);ap.add_argument('--package',type=Path);ap.add_argument('--image',default='termcraft-validation-v08:clean-input');a=ap.parse_args()
root=Path(__file__).resolve().parents[3]
out=root/'docs/parity-evidence/V08/clean-run';out.mkdir(parents=True,exist_ok=True)
pkg=(a.package or next((root/'target/package-games-v03-qualified').glob('68*/*'))).resolve()
image=subprocess.check_output(['docker','image','inspect',a.image,'--format','{{.Id}}'],text=True).strip()
name='termcraft-v08-clean-'+uuid.uuid4().hex[:10]
preamble='''set -euo pipefail
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
'''
script=preamble+a.script.read_text()
cmd=['docker','run','--name',name,'--label','org.termcraft.validation=v08-clean','--network','none','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--pids-limit','256','--memory','2g','--cpus','2','--tmpfs','/tmp:rw,nosuid,exec,mode=1777','--mount',f'type=bind,src={pkg},dst=/opt/termcraft,readonly','--mount',f'type=bind,src={out},dst=/evidence','--entrypoint','/usr/bin/bash',image,'-c',script]
t0=time.monotonic();result={'image':image,'package':str(pkg.relative_to(root)),'manifest_sha256':hashlib.sha256((pkg/'PACKAGE-MANIFEST.json').read_bytes()).hexdigest(),'command':cmd,'script_sha256':hashlib.sha256(script.encode()).hexdigest()}
try:
 p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=110)
 result.update(exit_code=p.returncode,seconds=round(time.monotonic()-t0,2));(out/f'{a.name}.log').write_text(p.stdout); print(p.returncode,p.stdout)
except subprocess.TimeoutExpired as e:
 result.update(exit_code='timeout',seconds=round(time.monotonic()-t0,2));(out/f'{a.name}.log').write_bytes(e.stdout or b'')
finally:
 subprocess.run(['docker','rm','-f',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
 result['owned_container_removed']=True;(out/f'{a.name}.json').write_text(json.dumps(result,indent=2)+'\n');(out/f'{a.name}-script.sh').write_text(script)

sys.exit(result['exit_code'] if isinstance(result['exit_code'],int) else 124)
