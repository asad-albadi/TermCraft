#!/usr/bin/env bash
set -eu
w=$(cd "$(dirname "$0")" && pwd)
cd "$w/source"
export CARGO_TARGET_DIR=$(realpath ../../../..) CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo fmt --all -- --check > "$w/evidence/fmt.log" 2>&1
cargo test --offline --locked --lib x11::xdmcp -- --nocapture > "$w/evidence/library.log" 2>&1
bash "$w/live.sh"
cargo test --offline --locked --bin termcraft-xdmcp-clipboard952 ui::xdmcp::tests -- --nocapture > "$w/evidence/ui.log" 2>&1
cargo build --offline --locked --bin termcraft-xdmcp-clipboard952 > "$w/evidence/build.log" 2>&1
