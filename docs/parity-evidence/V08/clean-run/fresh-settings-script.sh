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
launch fresh
click 565 100
shot fresh-settings
