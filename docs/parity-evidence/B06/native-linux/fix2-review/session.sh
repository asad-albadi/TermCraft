set -euo pipefail
umask 077
mkdir -p /tmp/runtime /tmp/store
chmod 700 /tmp/runtime
export DISPLAY=:77 XDG_RUNTIME_DIR=/tmp/runtime LIBGL_ALWAYS_SOFTWARE=1
xp='' ap=''
cleanup(){ for p in "$ap" "$xp"; do if [[ -n "$p" ]]; then kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; fi; done; echo cleaned > /evidence/cleanup-native2.txt; }
trap cleanup EXIT
Xvfb :77 -screen 0 1440x1000x24 -nolisten tcp > /evidence/xvfb2.log 2>&1 & xp=$!
sleep 1
/opt/termcraft/harbor-workspace --data-dir /tmp/store > /evidence/app2.log 2>&1 & ap=$!
sleep 5
win=$(xdotool search --pid "$ap" | head -1)
xdotool windowfocus --sync "$win"
echo "$ap $win" > /evidence/app2-window.txt
import -window root /evidence/000-start.png
i=1
while [[ ! -f /evidence/stop ]]; do
 f=$(printf '/evidence/action-%03d.sh' "$i")
 if [[ -f "$f" ]]; then bash "$f" > "${f%.sh}.log" 2>&1 || echo $? > "${f%.sh}.exit"; touch "${f%.sh}.done"; i=$((i+1)); fi
 sleep .15
done
cp /tmp/store/workspace.json /evidence/workspace2-after.json 2>/dev/null || true
