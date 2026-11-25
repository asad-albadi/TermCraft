# Reproduction

Run from this directory. Cargo is constrained to two jobs and the locked offline dependency set. The fixtures are owned test PTYs, helper child processes and the helper suite's loopback mocks. No app launch, physical serial hardware, native macOS run, host service integration, or Windows execution was requested or attempted.

The original FIX1 RED source snapshots remain in red-devices_tests.rs and red-serial.rs. red.log is the original saturated-channel failure and red-stats.log shows nonzero absolute counters at logical CREATE. The stronger GREEN saturation sequence additionally checks in-place retry and close-slot availability after refill.

```sh
S=source/engines/rdp/src
touch "$S/main.rs" "$S/serial.rs" "$S/devices.rs" "$S/devices_tests.rs"
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=../build cargo test --manifest-path source/engines/rdp/Cargo.toml --offline --locked -j 2 --bin termcraft-rdp-unix-control947 unix_event
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=../build cargo test --manifest-path source/engines/rdp/Cargo.toml --offline --locked -j 2
python3 source/tests/rdp-native-component/generate.py
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=../component-target cargo check --manifest-path source/tests/rdp-native-component/Cargo.toml --offline --locked -j 2 --target aarch64-apple-darwin
rustfmt --edition 2024 --config skip_children=true --check "$S/serial.rs" "$S/devices.rs" "$S/devices_tests.rs"
python3 freeze.py
```

The Mac component generator imports exact Unix serial source and extracts only the Drives portion of devices.rs. The Linux helper exercises the complete Channel path. The generated component source uses absolute private paths and is not part of either patch.
