#!/usr/bin/env bash
# Run on the already-authorized host. Preserve the earlier D02 host baseline logs.
set -u
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." || exit 1
termcraft_evidence=docs/parity-evidence/D02/providers
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
termcraft_result=0
sha256sum engines/rdp/src/{main,config,devices}.rs src/desktop/options.rs src/ui/rdp_settings.rs engines/rdp/Cargo.lock Cargo.lock > "$termcraft_evidence/host-source-sha256.txt"
termcraft_gate() {
    local termcraft_name="$1"
    shift
    "$@" > "$termcraft_evidence/host-$termcraft_name.log" 2>&1
    local termcraft_status=$?
    printf '%s: exit %s\n' "$termcraft_name" "$termcraft_status"
    if [[ "$termcraft_status" != 0 ]]; then termcraft_result=1; fi
}
termcraft_gate engine cargo test --offline --locked --manifest-path engines/rdp/Cargo.toml
termcraft_gate rdp-options cargo test --offline --test rdp_options -- --nocapture
termcraft_gate rdp-session cargo test --offline --test rdp_session -- --nocapture
termcraft_gate full cargo test --offline --no-fail-fast
exit "$termcraft_result"
