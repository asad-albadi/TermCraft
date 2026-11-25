#!/usr/bin/env bash
# Run through the authorized host route. Do not overwrite earlier host baselines.
set -u
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." || exit 1
termcraft_evidence=docs/parity-evidence/execution
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
termcraft_status=0
find src engines/rdp/src tests scripts -type f ! -path '*/__pycache__/*' -print0 | sort -z | xargs -0 sha256sum > "$termcraft_evidence/host-source-sha256.txt"
sha256sum Cargo.toml Cargo.lock engines/rdp/Cargo.toml engines/rdp/Cargo.lock PARITY_EXECUTION_QUEUE.json >> "$termcraft_evidence/host-source-sha256.txt"
termcraft_gate() {
    local termcraft_name="$1"
    shift
    "$@" > "$termcraft_evidence/host-$termcraft_name.log" 2>&1
    local termcraft_exit=$?
    printf '%s: exit %s\n' "$termcraft_name" "$termcraft_exit"
    if [[ "$termcraft_exit" != 0 ]]; then termcraft_status=1; fi
}
termcraft_gate queue python3 -m unittest discover -s scripts -p test_parity_queue.py
termcraft_gate engine cargo test --offline --locked --manifest-path engines/rdp/Cargo.toml
termcraft_gate full cargo test --offline --locked --no-fail-fast
exit "$termcraft_status"
