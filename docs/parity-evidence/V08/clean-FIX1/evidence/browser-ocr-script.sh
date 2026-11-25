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
for mode in minimal runtime; do
 tesseract "/evidence/browser-$mode.png" "/evidence/browser-$mode-ocr" -l eng --psm 11
done
python - <<'PY'
from pathlib import Path
import json
rows=[]
for mode in ['minimal','runtime']:
 text=' '.join(Path(f'/evidence/browser-{mode}-ocr.txt').read_text().split()).lower()
 assert 'the chromium process exited. check the installed browser and sandbox configuration.' in text,text
 rows.append({'mode':mode,'actual_ui_error_observed':True,'screenshot':f'browser-{mode}.png','ocr':f'browser-{mode}-ocr.txt'})
Path('/evidence/browser-ui-results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
PY
