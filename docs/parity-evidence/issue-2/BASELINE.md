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
- `cargo +1.96.1 test --locked --all-targets --no-fail-fast`: compilation succeeded; 273 library tests passed, 3 ignored, 3 library tests failed, and subsequent GUI targets reported fixture-dependent failures.

## Baseline failures retained as open

- VeNCrypt TLS fixture rejected its certificate as `CertificateUnknown`.
- Portable folder fixture and NFS fixture use raw names that macOS rejects with `Illegal byte sequence`.
- Several GUI tests fail downstream of unavailable/failed endpoint fixtures.

These failures are baseline/environment evidence, not native macOS qualification. They must not be relabeled as passing issue #2 acceptance criteria.

## Native helper verification

- `cargo test --locked --manifest-path engines/rdp/Cargo.toml`: passed, 27/27.
- The macOS preallocation test now verifies logical EOF, idempotence, and data preservation without assuming a particular filesystem's physical block accounting.
