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
cat /opt/termcraft-validation-runtime-packages.txt > /evidence/clean-runtime-image-packages.txt
python - <<'PY'
from pathlib import Path
import json,ctypes,os
assert not Path('/usr/bin/chromium').exists()
assert not Path('/usr/bin/mosh').exists()
libc=ctypes.CDLL(None,use_errno=True); namespace_result=libc.unshare(0x10000000); namespace_errno=ctypes.get_errno()
Path('/evidence/clean-runtime-namespace.json').write_text(json.dumps({'unshare_CLONE_NEWUSER_return':namespace_result,'errno':namespace_errno,'error':os.strerror(namespace_errno)},indent=2)+'\n')
Path('/evidence/clean-runtime-system-clients-absent.json').write_text(json.dumps({'system_chromium_absent':True,'system_mosh_absent':True,'perl':Path('/usr/bin/perl').exists(),'openssh':Path('/usr/bin/ssh').exists()},indent=2)+'\n')
PY
