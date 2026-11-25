set -euo pipefail
umask 077
mkdir -p /tmp/wayland /tmp/gui-store
chmod 700 /tmp/wayland
export DISPLAY=:77 XDG_RUNTIME_DIR=/tmp/wayland
ap='' xp='' hp=''
cleanup() { for p in "$ap" "$hp" "$xp"; do if [[ -n "$p" ]]; then kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; fi; done; }
trap cleanup EXIT
cat > /tmp/offline.html <<'HTML'
<!doctype html><html><title>V08 Offline Browser</title><body style="background:#fff;color:#111;font:30px sans-serif"><h1>V08_NATIVE_BROWSER_OK</h1><input id="entry" placeholder="Type browser proof here" autofocus style="width:90%;font:28px sans-serif"><p id="result"></p><script>document.querySelector('#entry').addEventListener('input',e=>{document.querySelector('#result').textContent='Native typed: '+e.target.value;fetch('/receipt?value='+encodeURIComponent(e.target.value));});</script></body></html>
HTML
cp /evidence/browser-fixture.json /tmp/gui-store/workspace.json
python -m http.server 25380 --bind 127.0.0.1 --directory /tmp > /evidence/gui-http.log 2>&1 & hp=$!
Xvfb :77 -screen 0 1360x880x24 -nolisten tcp > /evidence/gui-xvfb.log 2>&1 & xp=$!
sleep 1
/opt/termcraft/harbor-workspace --data-dir /tmp/gui-store > /evidence/gui-app.log 2>&1 & ap=$!
sleep 4
win=$(xdotool search --pid "$ap" | head -1)
xdotool windowfocus --sync "$win"
/opt/termcraft/harbor-workspace --data-dir /tmp/gui-store --session 0123456789abcdef0123456789abcdef > /evidence/gui-session.log 2>&1
sleep 8
import -window root /evidence/gui-browser-before.png
xdotool mousemove --sync 600 263 click 1
xdotool type --clearmodifiers --delay 40 'V08_NATIVE_INPUT_OK'
sleep 2
import -window root /evidence/gui-browser-input.png
python /evidence/inspect-gui.py
# Native PTY interaction in the same actual app; shortcut and terminal-widget focus.
xdotool key ctrl+shift+t
sleep 2
xdotool mousemove --sync 650 500 click 1
xdotool type --clearmodifiers --delay 20 "printf 'V08_NAMESPACE_NATIVE_PTY_OK\\n' > /evidence/gui-pty-marker.txt"
xdotool key Return
sleep 2
import -window root /evidence/gui-pty.png
cat /evidence/gui-pty-marker.txt
cp /tmp/gui-store/workspace.json /evidence/gui-workspace-after.json
