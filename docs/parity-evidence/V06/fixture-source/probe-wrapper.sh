#!/bin/bash
set -euo pipefail
v06_probe_app=${V06_APP_BINARY:-$(dirname "$0")/../../debug/termcraft-v06-native-matrix}
exec "$v06_probe_app" "$@" 2> "$V06_REPORT/probe-stderr.txt"
