#!/usr/bin/env bash
set -euo pipefail
# No host DISPLAY, Wayland, bus or PipeWire fallback. Every daemon belongs to this process.
if [[ ${E03_PRIVATE_BUS:-} != 1 ]]; then
  exec env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS -u PIPEWIRE_REMOTE E03_PRIVATE_BUS=1 dbus-run-session -- "$0" "$@"
fi
fixture_runtime=$(mktemp -d)
chmod 700 "$fixture_runtime"
export XDG_RUNTIME_DIR="$fixture_runtime" PIPEWIRE_RUNTIME_DIR="$fixture_runtime" PIPEWIRE_REMOTE=e03-private-pipewire
export TERMCRAFT_TEST_WAYLAND_ISOLATED=1
export PIPEWIRE_CONFIG_DIR="$fixture_runtime"
export XDG_CONFIG_HOME="$fixture_runtime/config" XDG_CONFIG_DIRS="$fixture_runtime/config-dirs"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CONFIG_DIRS"
cp "$(dirname "$0")/wayland-pipewire.conf" "$fixture_runtime/e03.conf"
cp "$(dirname "$0")/wayland-pipewire-client.conf" "$fixture_runtime/client.conf"
pipewire -c e03.conf >"$fixture_runtime/pipewire.log" 2>&1 &
fixture_pw_pid=$!
fixture_link_pid=
fixture_source_pid=
cleanup() {
  fixture_test_status=$?
  if [[ -n $fixture_link_pid ]]; then kill "$fixture_link_pid" 2>/dev/null || true; wait "$fixture_link_pid" 2>/dev/null || true; fi
  if [[ -n $fixture_source_pid ]]; then
    kill "$fixture_source_pid" 2>/dev/null || true
    if wait "$fixture_source_pid" 2>/dev/null; then echo "Owned synthetic source exited: 0"; else
      fixture_source_status=$?
      echo "Owned synthetic source exit: $fixture_source_status"
      [[ $fixture_source_status == 143 ]] || fixture_test_status=1
    fi
  fi
  kill "$fixture_pw_pid" 2>/dev/null || true
  if wait "$fixture_pw_pid" 2>/dev/null; then echo "Owned private PipeWire daemon exited: 0"; else echo "Owned private PipeWire daemon exit: $?"; fixture_test_status=1; fi
  cat "$fixture_runtime/source.log" "$fixture_runtime/pipewire.log"
  rm -rf "$fixture_runtime"
  exit "$fixture_test_status"
}
trap cleanup EXIT
for fixture_attempt in {1..100}; do [[ -S $fixture_runtime/e03-private-pipewire ]] && break; sleep .02; done
"${E03_SYNTHETIC_SOURCE:?Set the explicitly built private synthetic-source executable}" >"$fixture_runtime/source.log" 2>&1 &
fixture_source_pid=$!
for fixture_attempt in {1..100}; do
  pw-dump >"$fixture_runtime/objects.json"
  if python3 - "$fixture_runtime/objects.json" >"$fixture_runtime/node-ids" <<'PY_IDS'
import json,sys
nodes=[o for o in json.load(open(sys.argv[1])) if o.get('info',{}).get('props',{}).get('node.name')=='e03-synthetic']
if len(nodes)!=1: sys.exit(1)
print(nodes[0]['id'],nodes[0]['info']['props']['object.serial'])
PY_IDS
  then break; fi
  sleep .02
done
read -r E03_NODE E03_SERIAL <"$fixture_runtime/node-ids"
export E03_NODE E03_SERIAL
# No session manager, device monitor or permission agent is started. Link only the
# named synthetic source to the provider's named input on this private daemon.
(
  while kill -0 "$fixture_pw_pid" 2>/dev/null; do
    pw-link e03-synthetic:capture_1 harbor-vnc-portal-capture:input_1 2>/dev/null || true
    sleep .04
  done
) &
fixture_link_pid=$!
"$@"
