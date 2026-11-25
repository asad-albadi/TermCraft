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
