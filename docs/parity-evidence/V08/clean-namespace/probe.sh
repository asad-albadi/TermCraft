set -euo pipefail
umask 077
printf 'outer private namespace\n'
id
cat /proc/self/status
for ns in /proc/self/ns/*; do printf '%s ' "$ns"; readlink "$ns"; done
cat /proc/self/mountinfo > /evidence/mountinfo.txt
cat /proc/self/uid_map
cat /proc/self/gid_map
cat /opt/termcraft-validation-runtime-packages.txt > /evidence/runtime-packages.txt
printf '<!doctype html><html><title>V08 Offline Sandbox</title><body>V08_SANDBOX_OFFLINE_OK</body></html>' > /tmp/offline.html
browser=/opt/termcraft/browser/linux-x86_64/chromium
"$browser" --version
set +e
timeout 25 "$browser" --headless --disable-gpu --disable-dev-shm-usage --no-first-run --no-default-browser-check --user-data-dir=/tmp/headless-profile --dump-dom file:///tmp/offline.html > /evidence/headless-dom.txt 2> /evidence/headless-stderr.log
rc=$?
printf 'headless exit=%s\n' "$rc"
set -e
if [ "$rc" != 0 ]; then cat /evidence/headless-stderr.log; exit 3; fi
grep -q V08_SANDBOX_OFFLINE_OK /evidence/headless-dom.txt
# Keep a genuine browser alive for process-level sandbox inspection.
"$browser" --headless --disable-gpu --disable-dev-shm-usage --no-first-run --no-default-browser-check --user-data-dir=/tmp/inspect-profile --remote-debugging-port=19222 file:///tmp/offline.html > /evidence/inspect-browser.log 2>&1 & bp=$!
trap 'kill "$bp" 2>/dev/null || true; wait "$bp" 2>/dev/null || true' EXIT
sleep 4
python - <<'PYS'
import json,os
from pathlib import Path
rows=[]
for p in Path('/proc').iterdir():
 if not p.name.isdigit():continue
 try:
  cmd=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode()
  if '/opt/termcraft/browser/' not in cmd:continue
  rows.append({'pid':int(p.name),'cmdline':cmd,'status':(p/'status').read_text(),'namespaces':{n.name:os.readlink(n) for n in (p/'ns').iterdir()},'uid_map':(p/'uid_map').read_text(),'gid_map':(p/'gid_map').read_text()})
 except (FileNotFoundError,PermissionError,ProcessLookupError) as e:rows.append({'pid':int(p.name),'error':str(e)})
Path('/evidence/browser-processes.json').write_text(json.dumps(rows,indent=2)+'\n')
print('browser process records',len(rows))
PYS
kill "$bp"; wait "$bp" || true
trap - EXIT
