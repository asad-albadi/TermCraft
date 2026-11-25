# Native Apple Silicon evidence for issue #2

The host ran macOS 26.6.2 (25G83) on arm64 with macOS SDK 26.2 from Command
Line Tools, Apple clang 17.0.0 (clang-1700.6.3.2), and Rust 1.96.1.
The package was built with `MACOSX_DEPLOYMENT_TARGET=14.0`. The SDK and
deployment setting establish a build target. macOS 14 runtime remains untested.

## Core package

Source commit: `ebe40a693d8740329b33f33309c0a422cec6cb9d`.

| Item | SHA-256 |
| --- | --- |
| arm64 ZIP | `0d760eb3df93c7ddc255a16d2885925e4a02f704f9d0b196762a3b595322ba2b` |
| Frozen source inputs | `4b3df42534ccb0765cbbb90bb199bad5ea4eca1f7b16a6b17dfec66f418fde00` |
| Rust GUI | `3fe335966cf3edb801cd88ce4d4da17e34f5c116fefd65c7785bb7294287894c` |
| RDP helper | `467908158f4adeba4ffd7f9f50ebcad2ec4ba6f0e6ec760a0822588ef83c6ffa` |
| GUI Cargo.lock | `505f6d86e9b4d71504fec28af55ca21fdcd7b745071f7052a01464eaa0f0f511` |
| RDP Cargo.lock | `117db70a720c3cd3410f8454d28804c642eca478d9ebc6415fd6f24a39400eea` |
| RDP peer fixture Cargo.lock | `dafaf0ececa65f9d55ebd7340e9fb2d90ae42c710a3bd8a8ce456fb66f1bc6a6` |

`python3 scripts/package-release.py --output DIST --build-root BUILD`
built the core archive from separately locked GUI and RDP source snapshots.
`python3 scripts/package-check-startup.py ARCHIVE --evidence DIRECTORY`
verified 982 payload hashes, an empty-PATH launch through the relocated native
launcher, a 1360×880 GUI frame and the RDP helper wire greeting. A real
loopback RDP session sent a TPKT negotiation header. The live helper's
executable hash matched the packaged helper. The application preserved the
saved session and checked workspace preferences. See [desktop lifecycle](app-lifecycle.md)
for the native registration, replacement, vault and event observations.

Three packaged startup runs used isolated workspace preferences for System,
Light and Dark. All produced 1360×880 frames. Mean grayscale luminance was
31.14, 243.85 and 31.13 respectively. System followed Dark on this desktop.
This covers startup appearance on this host. Live OS theme transitions,
detached windows and physical display scaling still need separate evidence.

## Software qualification

- `cargo +1.96.1 test --locked --all-targets --no-fail-fast` on the native
  host, with four test threads, Pillow and an absolute, resolved temporary
  directory. The run on `ebe40a6` reported 911 passes, zero failures and
  ten ignored across 74 targets. The separate XQuartz run executes one of the
  ignored tests. Linux-only test targets ran zero cases on macOS.
- `cargo +1.96.1 test --locked --manifest-path engines/rdp/Cargo.toml`
  passed 27 helper tests, with zero failures or ignored tests. The standalone
  Linux PCSC worker integration target ran zero tests on macOS.
- The generated `tests/rdp-native-component` suite passed 13 tests.
  The `pcsc-test-macos` fixture suite in `tests/rdp-smartcard-native-component`
  passed 11 cases, including the parent-death audit. These are controlled
  provider tests and do not include a physical PCSC card or printer.
- Installed Google Chrome 158.0.8127.88 passed all four browser integration
  tests with `TERMCRAFT_REQUIRE_CHROMIUM=1`. The isolated home exposed only
  the existing browser through an `Applications` symlink. Native window
  sizing previously produced a 500-pixel-wide surface for a 300-pixel
  request. The separate CSS viewport and visible-surface resize passed the
  responsive input, history, proxy consent and untrusted TLS assertions.
- The separately provisioned htop run passed all three terminal program
  tests with `TERMCRAFT_TEST_HTOP` and `--include-ignored`, including live
  resize and alternate-screen restoration.
- FTPS passed 12 tests, including the original active-peer rejection.
  OpenSSH agent tests passed all eight. Actual Rust/RDP and VeNCrypt
  fixture peers use fresh CA-signed localhost certificates.

The initial failures had distinct causes. Darwin does not bind an
unconfigured `127.0.0.2`, and its accepted sockets can inherit nonblocking
mode. A real second local address kept the FTPS foreign-peer assertion.
Darwin's OpenSSH server is at `/usr/sbin/sshd`. Session fixtures use canonical
paths and short Unix sockets. The SCP helper now passes Python's supported
`F_GETPATH` buffer size, and SFTP preserves the original mode's type bits
when recognizing symlinks. HTTP streams set bounded blocking I/O before
sending responses. Bash 3.2 prompt hooks encode signed bytes as unsigned
octets. Tests still cover byte filenames on filesystems that accept them.

## Open qualification

The approved core package scope and installed-browser observations are
separate from a distributable bundled Chromium payload. Native XQuartz
candidate details are in [the private-engine ledger](xquartz-build.md).
The combined XQuartz trust gate remains closed. Intel builds and packages,
actual Finder Services activation and a version-changing upgrade, Direct and
Indirect XDMCP on native endpoints, and installed clean-machine checks on
macOS 14 remain open. Physical printer, smart-card and serial devices, real
adapter transitions, external displays/GPU rendering, and Screen Recording
and Accessibility consent, denial and revocation require additional fixtures
or operator evidence.
