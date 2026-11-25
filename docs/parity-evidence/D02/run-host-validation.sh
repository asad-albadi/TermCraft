#!/usr/bin/env bash
# Run on the authorized Linux host, outside the restricted Codex execution runner.
set -u
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." || exit 1
python3 docs/parity-evidence/D02/listener_probe.py > docs/parity-evidence/D02/host-listeners.log 2>&1
if [[ $? != 0 ]]; then
    cat docs/parity-evidence/D02/host-listeners.log
    exit 1
fi
termcraft_result=0
CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_options -- --nocapture > docs/parity-evidence/D02/host-rdp-options.log 2>&1 || termcraft_result=1
CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_session -- --nocapture > docs/parity-evidence/D02/host-rdp-session.log 2>&1 || termcraft_result=1
CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast > docs/parity-evidence/D02/host-full.log 2>&1 || termcraft_result=1
exit "$termcraft_result"
