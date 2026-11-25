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
export XDG_DATA_HOME=/tmp/install-data
export XDG_CONFIG_HOME=/evidence/install-config
mkdir -p "$XDG_CONFIG_HOME/harbor" "$XDG_DATA_HOME/termcraft/desktop"
cp /evidence/upgrade-current-restarted-store.json "$XDG_CONFIG_HOME/harbor/workspace.json"
printf 'unrelated-file-must-survive\n' > "$XDG_DATA_HOME/termcraft/desktop/unowned.txt"
printf 'preserve-defaults\n' > "$XDG_CONFIG_HOME/mimeapps.list"
python /opt/termcraft/desktop-integration.py install
python /opt/termcraft/desktop-integration.py status > /evidence/clean-install-status.log
cp "$XDG_DATA_HOME/termcraft/desktop/owned.json" /evidence/clean-install-owned.json
"$XDG_DATA_HOME/termcraft/desktop/payload/harbor-workspace" --data-dir "$XDG_CONFIG_HOME/harbor" --screenshot /evidence/clean-installed-startup.png
cp "$XDG_CONFIG_HOME/harbor/workspace.json" /evidence/clean-installed-store.json
python /opt/termcraft/desktop-integration.py uninstall
python /opt/termcraft/desktop-integration.py status > /evidence/clean-uninstall-status.log
python - <<'PY'
from pathlib import Path
import os
source=Path('/evidence/upgrade-current-restarted-store.json').read_bytes()
assert Path(os.environ['XDG_CONFIG_HOME']+'/harbor/workspace.json').read_bytes()==source
assert Path(os.environ['XDG_CONFIG_HOME']+'/mimeapps.list').read_text()=='preserve-defaults\n'
assert Path(os.environ['XDG_DATA_HOME']+'/termcraft/desktop/unowned.txt').read_text()=='unrelated-file-must-survive\n'
assert not Path(os.environ['XDG_DATA_HOME']+'/termcraft/desktop/payload/harbor-workspace').exists()
print('INSTALLED_STARTUP_UNINSTALL_WORKSPACE_DEFAULTS_UNOWNED_PRESERVED')
PY
