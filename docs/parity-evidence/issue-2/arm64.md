# Native Apple Silicon evidence for issue #2

The host ran macOS 26.6.2 (25G83) on arm64 with macOS SDK 26.2 from Command
Line Tools, Apple clang 17.0.0 (clang-1700.6.3.2), and Rust 1.96.1.
The package was built with `MACOSX_DEPLOYMENT_TARGET=14.0`. The newer package
below was also exercised on macOS 14.8.7.

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

## Arm64 package with private XQuartz

Source commit `c7dc7b7331c5743f63f8f285f9110aefaebc7e31` includes current
`main` at `cf7ee4d2f1373f18302a0f083bf3d4a3860b42f6`. The same SDK,
compiler, Rust and lockfile hashes listed above were used.

| Item | SHA-256 |
| --- | --- |
| `TermCraft-0.1.0-macos-aarch64-xquartz.zip` | `2241340fdf4d3d2aca0a8bdb3bc8bb7d3e1c57fb155c841c568de7b6a74edea8` |
| Frozen source inputs | `5e11caa8f31e3db7dc5da86502a6be0401481346a573ad13c0c7ddb4e96afa2c` |
| Rust GUI | `2adc3642f12e08c8a30f854c903afb8e5aa561acfd04f126951be7e8168de9dc` |
| RDP helper | `467908158f4adeba4ffd7f9f50ebcad2ec4ba6f0e6ec760a0822588ef83c6ffa` |
| Objective-C launcher | `9522fbc2b3f2007b32ff7f6d55982ce036643b1941f3efacbc294abd32c0c792` |
| Compiled XQuartz inventory | `0fd1bb3ffd162962691d4b7c0b6283b53e448b6a50a51c502c86074b5317f171` |

`MACOSX_DEPLOYMENT_TARGET=14.0 python3 scripts/package-release.py --output DIST --build-root BUILD --xquartz-runtime CANDIDATE`
built the GUI and independently locked RDP helper. The packager checked every
file against the compiled inventory and verified the staged XQuartz bundle and
Mach-O signatures. The app contains 1,800 hashed payload files, including 818
private XQuartz files. `xcrun vtool -show-build` reported 14.0 for the GUI,
helper and launcher. All 14 XQuartz Mach-O outputs report the same floor.
`python3 scripts/package-check-startup.py ARCHIVE --evidence DIRECTORY`
verified the relocated archive with an empty PATH, a 1360×880 GUI frame, a
live manifest-matching RDP helper and receipt of its initial RDP negotiation.

The packaged production XQuartz owner and managed SSH/X tests both passed
with a 256-file-descriptor limit. The previous snapshot held 818 read handles
and failed with `EMFILE` at Finder's limit. The Windows snapshot keeps its
write/delete-denying handles. The Unix snapshot releases each file handle after
verification and checks for cancellation between files. The low-limit replay
observed pre-cancellation, a new process group and cleanup of the socket,
authority, process and snapshot. The app's probe returned 1920×1050, GLX 1.4
and RandR 1.5. Core X11 pixmap readback returned the drawn white and black
pixels, and two authenticated X11 clients exchanged a UTF-8 selection.
The SSH fixture negotiated X11 setup and answered GetInputFocus over the same
authenticated login. Host clipboard synchronization, GLX readback and RandR
mode transitions remain unqualified.

A fresh, host-only Tart 2.42.0 guest ran Apple Silicon macOS 14.8.7 (23J520).
The guest image was `ghcr.io/cirruslabs/macos-sonoma-vanilla` at
`sha256:7ea0d508380b63f13c94ee72cd22ca3dd9ec5cdf4a5ae14b2ae2ce6a6603fba6`.
The ZIP matched the digest above before extraction. The same startup verifier
checked every file and observed a 1024×653 GUI frame plus live loopback RDP
negotiation with a manifest-matching helper. The 256-descriptor XQuartz
software probe read the expected pixels from a 1024×744 X display and
transferred the private UTF-8 selection. The wire probe rejected anonymous
and incorrect-cookie setup, accepted the owned cookie and verified cleanup.
This covers runtime startup on the tested macOS 14 patch level, with physical
devices and permission-mediated workflows tracked separately.

The arm64 rerun used `cargo +1.96.1 test --locked --all-targets --no-fail-fast`
with four test threads. It reported 911 passed, zero failed and 11 ignored
across 74 targets. The extra ignored case is the managed SSH/X test, which
passed separately against the verified package. The locked RDP helper passed
27 tests. With installed Fish 4.9.3, `shell_directory --include-ignored`
passed nine tests across Bash, Zsh and Fish. Installed Chrome and the native
terminal-program runs passed four and three tests respectively.

The Linux RDP failure
`devices_tests::drive_allocation_is_released_on_last_remote_close_and_disconnect`
was reproduced at `devices_tests.rs:589` on the original base and current
`main`, each with 81 passes and the same leaked-allocation assertion. The PR
run matched both baselines. [Original base run](https://github.com/asad-albadi/TermCraft/actions/runs/45638076373)
[Current main run](https://github.com/asad-albadi/TermCraft/actions/runs/45745650311)
[PR branch run](https://github.com/asad-albadi/TermCraft/actions/runs/45747731405)

Windows x64 GNU cross-checking found a PR-added Unix mode helper compiled
outside its platform guard. The checked raw-mode conversion now runs only on
Unix and preserves type bits. `cargo +1.96.1 check --locked --target x86_64-pc-windows-gnu --lib --bins`
passed. Adding `--all-targets` finds the same existing test-only import
error in `src/ui/layout.rs:255` on the branch and a clean archive of current
`main`. That test imports Unix-only fixture functions on Windows.

## Open qualification

The remaining Apple Silicon software work includes bundled Chromium with a
pinned native source and signed resource tree, Direct and Indirect XDMCP with
a native session-owned network and authentication path, and full Finder Services
activation. The AppKit provider passed literal folder and mixed-selection
checks. The programmatic Services invocation returned false and actual Finder
menu activation remains unverified. XQuartz deliberately disables upstream
host pasteboard sync, so X11 selection tests cover only the private display.
Public distribution approval and release automation replay remain open.

Run external-display/Retina scaling and GPU workflows on physical hardware.
Exercise physical CUPS printer, PCSC smart-card and serial devices, and real
adapter transitions. Screen Recording and Accessibility still need
human-operated grant, denial and revocation, including view-only/control and
held-input release on Stop.
