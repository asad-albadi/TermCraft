#!/usr/bin/env bash
set -euo pipefail
termcraft_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
termcraft_network=()
if [[ "${TERMCRAFT_OFFLINE:-0}" == 1 ]]; then termcraft_network=(--offline); fi
cargo build --locked "${termcraft_network[@]}" --manifest-path "$termcraft_root/engines/rdp/Cargo.toml" --target-dir "$termcraft_root/engines/rdp/target"
exec cargo run --locked "${termcraft_network[@]}" --manifest-path "$termcraft_root/Cargo.toml" -- "$@"
