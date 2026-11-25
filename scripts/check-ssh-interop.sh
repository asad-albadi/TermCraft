#!/bin/sh
set -eu
exec python3 "$(dirname "$0")/check-interop.py" V01 "$@"
