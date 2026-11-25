# Issue #2 Baseline

Recorded on 2026-11-25 from branch `issue-2-macos` at the current worktree.

## Host and source

- Host: macOS arm64 development host
- Toolchain: Rust 1.96.1
- Source base: `98b213d`
- Branch: `issue-2-macos`

## Compatibility repairs

- Converted encoded POSIX modes through checked `u16` boundaries before calling the pinned `rustix` APIs.
- Updated macOS `libc::openpty` call sites to pass the current API's mutable `termios`/`winsize` pointer types.
- Replaced unstable `AtomicUsize::try_update` calls with the stable `fetch_update` API used by the pinned Rust toolchain.
- Added a regression test for mode conversion overflow.

## Verification

- `cargo +1.96.1 test --locked mode_tests::encoded_modes_are_checked_before_rustix_conversion --lib`: passed, 1/1.
- `cargo +1.96.1 fmt --all -- --check`: passed after formatting.
- `cargo +1.96.1 test --locked --lib --no-fail-fast`: passed, 276/276, with 3 ignored.
- `cargo +1.96.1 test --locked --all-targets --no-fail-fast`: the library target passed 276/276; broader GUI/integration targets compiled but 26 targets reported environment-sensitive failures, including unavailable external SSH helpers and concurrent local-socket/resource limits.

## macOS fixture compatibility repairs

- VeNCrypt now uses a proper test CA and localhost leaf certificate chain, which passes the macOS platform verifier.
- Portable-folder and NFS raw-byte filename assertions are retained on platforms that support non-UTF-8 directory entries; macOS exercises the ordinary-name, symlink, and size-limit paths instead of attempting an invalid filename.
- The macOS workspace IPC boundary no longer requests unsupported socket `fchmod`; private directory, random namespace, and authenticated token protections remain in force. The same boundary is applied to the owned agent listener.

The broader target failures remain environment evidence, not native macOS qualification. They must not be relabeled as passing issue #2 acceptance criteria.

## Native helper verification

- `cargo test --locked --manifest-path engines/rdp/Cargo.toml`: passed, 27/27.
- The macOS preallocation test now verifies logical EOF, idempotence, and data preservation without assuming a particular filesystem's physical block accounting.

## Native XQuartz feasibility probe

- The pinned xorg-server archive matched `36db027489d6370ec5b1b6d07c8155c547193c2a6914fd617eb4311fb029257f`.
- Native Meson configuration reached Apple clang 17.0.0 on arm64 with the private-entry patch applied.
- Configuration stopped before compilation because `pkg-config` and the pinned `xorgproto` subproject closure are absent. This is an environment/toolchain blocker, not native qualification evidence.
