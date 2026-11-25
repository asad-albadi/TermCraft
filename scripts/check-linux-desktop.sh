#!/usr/bin/env bash
set -euo pipefail
v06_fixture=${V06_FIXTURE_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}
[[ -f "$v06_fixture/pins.json" && -x "$v06_fixture/run-private.sh" ]]
for v06_tool in bwrap dbus-run-session python grim import wl-paste; do command -v "$v06_tool" >/dev/null || { echo "Missing native prerequisite: $v06_tool" >&2; exit 2; }; done
python -I "$v06_fixture/verify.py" --prerequisites
if [[ ${1:---verify} == --run ]]; then
  export V06_APP_BINARY=${V06_APP_BINARY:-$v06_fixture/package-extracted/TermCraft-0.1.706-linux-x86_64-arch-xwayland/harbor-workspace}
  export V06_BUNDLE=${V06_BUNDLE:-$v06_fixture/package-extracted/TermCraft-0.1.706-linux-x86_64-arch-xwayland}
  export V06_REPORT_ROOT="$v06_fixture/runs/$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$V06_REPORT_ROOT"
  for v06_backend in x11 wayland; do
    for v06_scale in 1 1.5 2; do "$v06_fixture/run-private.sh" "$v06_backend" "$v06_scale"; done
  done
  "$v06_fixture/run-private.sh" wayland 1 engine
  exec python -I "$v06_fixture/verify.py" "$V06_REPORT_ROOT"
elif [[ ${1:---verify} == --verify ]]; then
  exec python -I "$v06_fixture/verify.py" "${V06_REPORT_ROOT:-$v06_fixture/package-reports}"
else echo 'Expected --run or --verify' >&2; exit 2; fi
