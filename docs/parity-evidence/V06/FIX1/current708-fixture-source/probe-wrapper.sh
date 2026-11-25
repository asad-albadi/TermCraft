#!/bin/bash
set -euo pipefail
exec "${V06_APP_BINARY:?}" "$@" 2> "$V06_REPORT/probe-stderr.txt"
