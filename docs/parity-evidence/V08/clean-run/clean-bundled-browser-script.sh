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
python -m http.server 25380 --bind 127.0.0.1 --directory /tmp > /evidence/clean-browser-http.log 2>&1 & hp=$!
trap 'kill "$hp" 2>/dev/null || true; wait "$hp" 2>/dev/null || true; cleanup' EXIT
launch browser
/opt/termcraft/harbor-workspace --data-dir /evidence/browser --session 0123456789abcdef0123456789abcdef
sleep 8
shot clean-bundled-browser
set +e
LD_LIBRARY_PATH=/opt/termcraft/browser/linux-x86_64/libraries /opt/termcraft/browser/linux-x86_64/chromium --version > /evidence/clean-browser-version.log 2>&1
version_rc=$?
printf 'browser --version exit=%s\n' "$version_rc"
