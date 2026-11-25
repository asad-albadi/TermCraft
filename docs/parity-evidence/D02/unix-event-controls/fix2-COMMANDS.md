# FIX2 reproduction

From this directory, using the existing private .947 helper identity:

```sh
S=source/engines/rdp/src
rustfmt --edition 2024 --config skip_children=true --check "$S/devices.rs" "$S/devices_tests.rs"
touch "$S/main.rs" "$S/devices.rs" "$S/devices_tests.rs"
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=../build cargo test --manifest-path source/engines/rdp/Cargo.toml --offline --locked -j 2 --bin termcraft-rdp-unix-control947 devices_tests::serial
python3 -u freeze.py
```
