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

The branch rebuild from `788439b3d10127ded70be134bf316071212b40e5`
produced ZIP SHA-256
`eb5ef97d63cc01cc64537d794171bd383611f74ba0455a7690b45e3486a3c4f8`,
frozen inputs
`0ec0abe3eac66b668f7be17a099a4443ef25d8b0950a975af7c3c163c6d2406c`
and GUI executable
`5351578ec1c0d1b22133a47ec33f167437669750aa631ec590560c3d177018e6`.
The launcher, RDP helper and XQuartz inventory hashes matched the previous
package. The startup verifier passed on the host and guest, including live
RDP negotiation and the packaged helper identity. The guest frame was
1024×656. The host XQuartz-owner and managed SSH/X tests also passed against
this ZIP under the 256-descriptor limit.

The arm64 rerun used `cargo +1.96.1 test --locked --all-targets --no-fail-fast`
with four test threads. It reported 911 passed, zero failed and 11 ignored
across 74 targets. The extra ignored case is the managed SSH/X test, which
passed separately against the verified package. The locked RDP helper passed
27 tests. With installed Fish 4.9.3, `shell_directory --include-ignored`
passed nine tests across Bash, Zsh and Fish. Installed Chrome and the native
terminal-program runs passed four and three tests respectively.

The full suite returned the same counts after the portable mode fix in
`ea9f46280823ad8a09f0c9182a74b15c2f566ea2`.

The native Dropbear 2026.94 fixture passed key verification, shell I/O and
a 90,000-byte SFTP round trip with a literal filename over one authenticated
connection. `scripts/prepare-dropbear-interop.sh` checked the upstream SHA-256
and pinned OpenPGP fingerprint before building. Darwin lacks `setresgid`.
The fixture build uses Dropbear's session-child user switch and omits Unix
socket forwarding. The signing-key import now uses a short private directory
to fit Darwin's socket path limit.

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

## Combined Apple Silicon qualification archive

`python3 scripts/package-release.py --build-root BUILD --output DIST
--xquartz-runtime XQUARTZ --macos-browser-fixture CHROMIUM_INPUTS` built the
locked GUI and independent RDP helper from source
`6b76c74145b9597108eb7773214cae9c0495cae5`. Build inputs used Rust 1.96.1,
SDK 26.2 and `MACOSX_DEPLOYMENT_TARGET=14.0`. The package manifest records
source snapshot `d23cf96605376b967c22dd281ef0b81f3d3f1f44e41d00f8f86481861321af3f`
and the opt-in GUI feature. The combined archive contains 2,139 hashed files
and five literal framework links. SHA-256 of
`TermCraft-0.1.0-macos-aarch64-xquartz-chromium-qualification.zip` is
`4cc6176ab255e144c90ba0b986bc8e6d825a6629ce6a150397080561b74aa692`.
Its unpacked Chromium app passed `codesign --verify --deep --strict` and
used its local ad-hoc resource seal. The package profile is `qualification`.

The earlier integrated archive from
`1bc64f7d37d6803b0839ec03c8ca26902dce3dec` had SHA-256
`23648b5da2b87e0abc23e2132846c62ea601ad6e9a7d2e8b0b2377490fcf5974`.
`python3 scripts/package-check-startup.py ARCHIVE --evidence DIRECTORY`
verified every file and link with relocated startup and live RDP negotiation
on macOS 26.6.2 and Apple Silicon Sonoma 14.8.7. The rebuilt archive replay
also verified the relocated GUI and live manifest-matching helper on both
versions. Comparing the initial and final archive inventories found only
`harbor-workspace` changed. The five symlink texts, RDP helper, launcher,
Chromium and private XQuartz runtime bytes all matched. Both rebuilt archive
browser protocol runs passed 4/4. Finder Services on Sonoma dispatched both
pasteboard formats to the literal terminal working directories and unregistered
the fixture. `vtool -show-build` confirmed arm64 and a 14.0 minimum for the GUI,
RDP helper and launcher. The bundled Chromium executable is arm64 with minimum
13.0 and SDK 27.0.

The native `macos_xdmcp` harness passed four owned-peer checks at the
256-descriptor limit against the relocated combined package. Two separate
real Debian xdm tests passed at the same limit. Direct opened an authenticated
session at `:184`. Indirect authenticated at `:185`, handed off through the
real Debian chooser callback, and rotated the display to a fresh XQuartz
process. The runtime removed the listener, authority and owned process.
See the [XDMCP ledger](xdmcp-macos.md).

AppKit Services registration, dispatch for both pasteboard formats, literal
folder terminal working directories and unregistration passed with the
combined archive on macOS 14.8.7. See [application lifecycle](app-lifecycle.md).

The default workspace command used the separately locked owned RDP
peer and runtime through `TERMCRAFT_V03_RDP_FIXTURE` and
`TERMCRAFT_V03_RDP_ENGINE`. A fixture had decoded an SFTP command through
a fixed-width terminal text dump. A long temporary path wrapped the hex
output. Capturing the raw PTY command in an owned file now checks every
byte and passed with the long path. A later run filled the internal
filesystem while creating short `/private/tmp` SSH fixtures. Moving the
generated RDP build cache to the external test volume recovered 2.5 GiB
with links at the original cache paths. The SCP and services boundary
replays passed 20/20 and 16/16 after recovery. The final
`cargo +1.96.1 test --locked --all-targets --no-fail-fast` run reported
915 passed, zero failed and 15 ignored across 75 targets at source
`15b7971fa4c8a0a513390ee011f0fa7c80b8c4ee`. The package tests ran 22
cases and skipped 24. The hosted macOS failures reproduced with Python
fixtures under the OS-provided symlinked `/var` temp root. Resolving only
the owned fixture roots passed on the default macOS temporary location,
including the extractor’s symlink rejection cases. The Windows GNU library and executable cross-check
also passed. `cargo +1.96.1 check --locked --target
x86_64-unknown-linux-gnu --lib --bins` passed with a Zig cross C compiler
after `folder_archive::unix` switched to rustix’s `RawMode`.
The default macOS packaging contract passed after its fixture path fix.
The layout test also checks that prebuilt macOS inputs do not claim a build
SDK. A hosted Linux run reproduced the pre-existing iperf2 ABI mismatch on
Ubuntu 24.04. The failing binary has SHA-256
`570328443a3fbeb426dba301e2c892b3026645c2d7b3da8bc2fea9688978a834` on
both main and the PR and requires `GLIBC_2.43`.
[Hosted Linux job](https://github.com/asad-albadi/TermCraft/actions/runs/45755486304)

## Hosted unsigned core release replay

The arm64 job for source `15b7971fa4c8a0a513390ee011f0fa7c80b8c4ee` ran
macOS 15.8.2 (24H232), Xcode 16.4 (16F6) and SDK 15.5. The locked RDP suite
passed 27 tests and the packaging contract passed. The uploaded core ZIP
`TermCraft-0.1.0-macos-aarch64.zip` had SHA-256
`c591bb1e53ac143540b29703bda69b66e80601a340fe2f5371a829536fada9f4`.
Its downloaded bytes matched the runner’s checksum. The relocated startup
evidence records a 1024×653 frame, live loopback RDP negotiation and a
manifest-matching helper under an empty PATH. This workflow produces the
unsigned core archive. The optional Chromium and private XQuartz replay
used the separately qualified local archive above.
[Hosted arm64 job](https://github.com/asad-albadi/TermCraft/actions/runs/45755613396/job/139693423728)
[Uploaded core artifact](https://github.com/asad-albadi/TermCraft/actions/runs/45755613396/artifacts/15088466715)

## Managed XQuartz display and clipboard replay

The combined archive built from
`bf3916bd555a3844530871ffd2f0c8349e46eea0` (snapshot
`6fba56c1876dcfc23ff3a0bd359e0c8311dc7b18330fcafda1ce7837d6a17db9`)
has SHA-256 `ce4769aea630dee5f9c9cf66ffcac46957f47e144e390897fc00860e0b8e61c7`.
It passed relocation and manifest-matching live RDP startup on macOS 26.6.2
and 14.8.7. Rust 1.96.1, Apple SDK 26.2 and deployment target 14.0 were used.

The native capability child verified scratch pixmap readback before publishing
the display. A live owned peer exchanged UTF-8 selections in both directions,
verified ownership release and rejected invalid text and data over 64 KiB.
The disposable Sonoma session exercised the actual workspace worker at a
256-descriptor limit. It read the native pasteboard on an explicit request,
supplied the text to a private X11 peer, and emitted the app copy command
with the exact peer text on request. The X11 selection change alone left the
native pasteboard unchanged. Stopping the workspace display retired the
clipboard connection. These controls belong to the managed X server.
See the [XQuartz ledger](xquartz-build.md) and [provider replay](providers.md).

The follow-up at `8391d41` assigns a fresh X11 selection owner and timestamp
to each copy. The live peer checks ICCCM `TIMESTAMP` and `MULTIPLE` replies
and rejects a request for a superseded owner. A disposable Sonoma GUI
session verified actual reads and writes to the AppKit general pasteboard,
including a delayed X11 reply that left a newer native clipboard value
intact. Native source reads use a single background worker, while Stop and
quit can cancel the X11 owner independently.

The updated archive from `3855f7040679d243cdd03de21a849817417f21a7` includes
the System CUPS fix. Its SHA-256 is
`fa23d6eabc09d50852bb3ef8115d934421368028f22141c5103cca5da7638c82`.
Snapshot `33a45a9afcde4774ba83427f48970569950b02a36b77b1d079c4d3ecfea25e3e`
matched all 607 Git source inputs. The package lists 2,139 hashed files and
five link targets. Its only payload hash change from the `8391d41` archive
(`1fa61e41ef2f0f1e190ea4fc27a164d2b90b42f41b0e25ec8ca94304f6448776`)
is the RDP helper. The new build used the same Rust 1.96.1, SDK 26.2 and
deployment target 14.0. The default qualification profile and native runtime
approval gates were retained.

`python3 scripts/package-check-startup.py ARCHIVE --evidence DIRECTORY`
passed every archive digest and link plus relocated GUI and manifest-matching
live RDP startup on macOS 26.6.2 and Sonoma 14.8.7. The final Sonoma run
passed strict nested Chromium signature verification and the four bundled
browser protocol cases, opened both literal Finder folders through AppKit,
and spooled the known 78-byte job through the packaged helper. The owned
Direct, Indirect and silent-manager cancellation XDMCP tests also passed
with this archive at 256 descriptors on the host.

At `3855f70`, `cargo +1.96.1 test --locked --all-targets --no-fail-fast`
passed 917 tests, failed zero and ignored 17 across 75 targets. The run used
the separately built RDP evaluation peer and helper, Python fixture packages,
`RUST_TEST_THREADS=4` and a short owned `TMPDIR`. The locked standalone RDP
helper suite passed 28 tests with zero failures or ignored cases.

An initial `25602a1` all-target run hit a Cron fixture race. The test waits
for the shell-redirection file to exist and can read it before the shell
writes the PID. The observed parse error was `ParseIntError { kind: Empty }`.
`src/services/cron/tests.rs` is byte-identical to `main` at `cf7ee4d`; the
same full command passed on replay. Linux x86-64 GNU `cargo +1.96.1 check
--locked --target x86_64-unknown-linux-gnu --tests` also passed with the Zig
cross C compiler. Windows GNU `--lib --bins` passed. Hosted Linux continues
to expose the RDP drive allocation assertion reproduced on `main`.
[Hosted Linux PR job](https://github.com/asad-albadi/TermCraft/actions/runs/45765725145)
[Hosted main reproducer](https://github.com/asad-albadi/TermCraft/actions/runs/45745650311)

## Open qualification

The pinned Apple Silicon Chromium is usable for local qualification. Public
distribution requires a maintained arm64 candidate with complete source-matched
notices and reviewed release signing. The tested fixture embeds placeholder
credits. See the [Chromium evidence](chromium-arm64.md).

The private XQuartz profile provides core rendering and private UTF-8 X11
selection exchange. It reports no active RandR outputs or modes on Sonoma and
disables indirect GLX and automatic host pasteboard synchronization. Explicit
UTF-8 clipboard transfers through the managed display passed. Direct GLX
needs a compatible Apple-DRI client stack for separate qualification.
Visible Finder Services invocation and visible XDMCP chooser selection
remain operator checks. Public release automation and distribution
approval remain separate from this development archive.

Run external-display/Retina scaling and GPU workflows on physical hardware.
Exercise physical CUPS printer, PCSC smart-card and serial devices, and real
adapter transitions. Screen Recording and Accessibility still need
human-operated grant, denial and revocation, including view-only/control and
held-input release on Stop.
