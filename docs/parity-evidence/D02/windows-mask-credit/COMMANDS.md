Run from this private job directory, with the installed default Rust 1.99.0 stable toolchain.
The first command pair intentionally returns 101 for two behavioral baseline assertions.
No Windows command below runs a Windows binary.

```sh
python3 generate-admission.py base
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/admission-build" cargo test --manifest-path admission/Cargo.toml --offline --locked -j 2

python3 generate-admission.py source
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/admission-build" cargo test --manifest-path admission/Cargo.toml --offline --locked -j 2
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/admission-build" cargo check --manifest-path admission/Cargo.toml --offline --locked -j 2 --target x86_64-pc-windows-msvc --tests

python3 source/tests/rdp-native-component/generate.py
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/windows-component-build" cargo check --manifest-path source/tests/rdp-native-component/Cargo.toml --offline --locked -j 2 --target x86_64-pc-windows-msvc --tests

CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$(realpath ../../..)/target" cargo test --manifest-path source/engines/rdp/Cargo.toml --offline --locked -j 2
rustfmt --edition 2024 --check source/engines/rdp/src/{devices.rs,serial_windows.rs,serial_windows_control.rs} admission/src/admission_tests.rs
patch --dry-run -p1 -d base < integration.patch
sha256sum --check evidence.sha256
```
