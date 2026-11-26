# Windows x64 qualification for issue #1

## Continuation on main cf7ee4d

- The Windows BusyBox-w32 archive staged successfully with SHA-256
  `4649824ec37541b1939b0c18805b6475adba268bbbd743525413e033803cf081`.
  The pinned upstream PE, source and license checksums are unchanged. An
  independent audit compared the full generated member list and checked
  every one of the 24 applet files and the license against their source
  checksums. Native installation, a drive-qualified path with spaces, and
  a real ConPTY command using the private home all passed in
  `cargo +1.96.1 test --locked --test package_manager -- --include-ignored`
  (2 passed).
- The opt-in `packaged_windows_registration_dispatches_and_guards_uninstall`
  test installed real per-user handlers pointing to the staged executable.
  An isolated child supplied its own LOCALAPPDATA. Windows shell routing
  rejected a malformed TermCraft URL and delivered a valid bookmark to the
  child fixture's IPC receiver. The packaged uninstaller guard refused removal
  while the handlers existed and passed after the fixture removed them.
- The rebuilt private X candidate completed Direct and Indirect XDMCP with
  a controlled loopback peer. The Indirect case selected the peer through
  the candidate's own Win32 chooser. Both modes completed REQUEST, ACCEPT
  and MANAGE, refused anonymous X setup, accepted the assigned cookie,
  returned the core keyboard mapping for `a` and advertised XKEYBOARD,
  GLX, RANDR and Composite. The owned provider test independently passed
  job, listener and embedded SSH forwarding cleanup (1 passed). The exact
  candidate `vcxsrv.exe` SHA-256 is
  `7d16a924201add49616b3a1aee3f942f369836be74747c82f52848c54745c443`.
  The staged inventory SHA-256 is
  `f23f73c1acef9b54ef7d2df3fec50770433e2966e30907828952729a38e2fb25`.
  Windows Authenticode validated the Microsoft signatures on the three
  app-local VC143 runtime DLLs at file version `14.44.35211.0`. The exact
  staged hashes are `0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9`
  for `msvcp140.dll`, `d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066`
  for `vcruntime140.dll` and `1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f`
  for `vcruntime140_1.dll`. Microsoft lists the VC redist directory under
  its Visual Studio license conditions. Distribution approval remains open.
  [Microsoft Visual Studio 2022 REDIST list](https://learn.microsoft.com/visualstudio/releases/2022/redistribution)
  The applied patch normalizes IPv4 and IPv6 addresses before formatting a
  bounded chooser label and checks the list selection before reading it.
  Application-owned XDMCP, layout switching, clipboard and graphical
  presentation are still separate qualification steps.
- The software-rendering fixture used [Mesa for Windows 26.3.1](https://github.com/pal1000/mesa-dist-win/releases/tag/26.3.1).
  The Windows release workflow checks the archive and the two extracted DLLs
  against pinned SHA-256 fingerprints. Local relocated startup observed the
  app load both fixture DLLs and reach the exact packaged RDP helper. The
  installed GUI test also passed with the same fixture. It removed the two
  temporary DLLs before checking uninstall. Both DLL copies are checked
  against their pins before startup. The release archive contains neither
  fixture DLL.
- The optional Unix ZIP was built from frozen source snapshot
  `65e3297ee0cf653bcbd28b70634afdb1a344551fe85d317b482ee518cf9c98ff`
  using `python scripts/package-release.py --platform windows --build-root
  target/windows-qualification/build --unix-cache target/p05-cache --output
  target/windows-qualification/continuation-unix` under Developer PowerShell.
  Its SHA-256 is
  `0cd6a13cff8cb585ec7e424e1764bc66c2715fce7bf7f19aef41e31db5de1b93`.
  The relocated, empty-PATH GUI used the exact adjacent helper
  `e835cb0559d6a9984616cdd8e7d3d289b21a1e545328a30fe516720101d86429`.
  The installer exercised two independently built GUI images
  `de7c1fc4bdc9f6ee07dd1e0f71d53d6b1ba7c6233dde574881583bc19ef55299`
  and `ffaef7c7093f46f078ea019869979a557a2283a7b57ccbf9638668c792840a61`
  and preserved an isolated workspace, encrypted vault and unrelated file.
- The exact helper from that ZIP passed
  `cargo +1.96.1 test --locked --test rdp_session -- --include-ignored`
  (11 passed). The independent TLS/NLA endpoint and the helper exchanged
  Unicode clipboard text in both directions using a private clipboard
  fixture. The replay also covered resize, input, certificate decisions,
  cancellation and transport cleanup.
- The Windows root suite at `98160ab` reported 606 passes, zero failures and
  26 ignored tests using `cargo +1.96.1 test --locked --all-targets --no-fail-fast`.
  The shell exposed OpenSSL 3.2.4 from Git for Windows. A first run without
  that directory on PATH reported four absent-executable errors in the VNC
  TLS certificate fixtures. All four passed on the full replay with OpenSSL
  available. The independently locked helper replay reported 35 passes.
- The isolated Windows vault fixture created, wrote and reopened a synthetic
  credential. Windows `Get-Acl` confirmed that the vault and the vault lock
  both had the current user as owner, inheritance disabled and one explicit
  `OWNER RIGHTS` FullControl entry. The test did not use a personal vault.
- The packaged GUI rendered three isolated workspaces using `--data-dir`
  and `--screenshot`, with saved System, Light and Dark preferences. Windows
  reported `AppsUseLightTheme=0`. The 1360 × 880 captures had the same
  background RGB `(19,25,23)` for System and Dark. Light rendered
  `(251,253,252)` at the same location. The test read the OS preference and
  left it unchanged. Live settings changes and DPI remain open.
- [Windows hosted run 45747315026](https://github.com/asad-albadi/TermCraft/actions/runs/45747315026/job/139666404323)
  built the unsigned x64 ZIP, verified help and the adjacent helper handshake
  and produced a native GUI screenshot using the pinned renderer. Normal
  startup exited before RDP negotiation with `Private file belongs to another
  user` when opening the new workspace dispatcher. New private files and the
  IPC pipe now specify the actual account as owner in their security
  descriptors at creation. Checking an existing file still rejects a
  foreign owner. The startup probe now reports early exit and its log before
  the socket deadline.
- At `2d548ca`, the full OpenSSL-enabled Windows root replay passed
  606 tests with zero failures and 26 ignored. The native dispatch and
  credential vault suites passed five tests each and the registry suite
  passed 22 with one opt-in case.
- The ownership-fix build from frozen source snapshot
  `dc66836098c147383803f0f2dc9bfa64c80a6b9776c2d48cac790d709d87b64e`
  produced the optional Unix Windows x64 ZIP SHA-256
  `34c38f750c629e7c4d77745b841601f6effd74706d17203a8f115bc327c8e447`.
  Its GUI SHA-256 was
  `500bbd886f3e46bf922794ff4e337da2612d7dfe2820fbca0071deeb26b8f535`.
  The adjacent helper SHA-256 was
  `9df84270e6333a3bf24ae28ad9c8ab42ad886d85fd3296ed90aa87184d3130e4`.
  Relocated startup with the exact helper and software renderer passed, as
  did the native installer replay with two different build images, all 11
  packaged-helper RDP endpoint tests and the opt-in ShellExecute/guard case.
- [Hosted Windows run 45750110857](https://github.com/asad-albadi/TermCraft/actions/runs/45750110857/job/139675574627)
  passed the package contract (1), IPC suite (5), vault suite (5), relocated
  GUI at 1024 × 768, adjacent-helper negotiation and installed startup,
  shortcuts, uninstall refusal, reinstall and cleanup. The hosted unsigned
  ZIP SHA-256 is
  `0a6932ac5d88cf0489a2232151afa2908e2aca2d3a1488b7cc212512c6b49c75`.
  The hosted installer used one build with two synthetic manifest versions.
- The local installer separately passed with original manifests from
  `0.1.0` and `0.1.1-qualification.20261126` builds. The newer independent
  locked build changed the GUI version in a copy of the frozen source
  and kept the RDP helper lock. Staging at the original long output path
  had previously failed on the transitive `rfd` license tree with
  WinError 206. A short internal directory with the original versioned ZIP
  root fixed the failure at the same output path. The rebuilt version ZIP
  SHA-256 is `e4b1acbac116ca9772105b88a3852591a50ae973cefbec23a45c0ff28e2bdc7c`.
  The ordered installer replay preserved the fixture workspace, preferences,
  encrypted vault and unrelated file, then removed both versions and their
  Start menu shortcuts.
- In another native X replay, a candidate startup failed the app's anonymous
  readiness check. The pinned server read binary Xauthority records in CRT
  text mode, which can truncate or translate bytes, and retained a host-based
  fallback when parsing failed. The private source now reads binary and
  requires protocol authentication on its Windows listeners. The exact source
  patch SHA-256 is
  `a5f68d6b08b143dea46eab205fee173aa3cf29db595245424d15c4b0eaa9f740`.
  The replacement server SHA-256 is
  `12fa366d4774c6ca01dca4f82af4d2c5cfe11c40ab79e4d325c647adafbeae45`,
  with candidate inventory SHA-256
  `2aca7f738620d8d67428abfc8345aea6123330b7a4bdf43fc8e1f83278639e52`.
  The offline source recipe re-applied the exact patch. The native probe
  accepted a valid cookie containing control-Z and CR/LF, rejected a
  cookie-free request, and passed both Direct and Indirect XDMCP again.
  The owned Rust provider replay also passed one test on this exact
  inventory, including an authenticated X window through embedded SSH, PID
  ownership and removal of the listener and private runtime. The Windows
  snapshot and runtime directories use explicit current-user ownership.
  [Microsoft CRT stream modes](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/fopen-wfopen)
- The Windows x64 application routes Direct and Indirect XDMCP through its
  owned native lease. It requires explicit consent, a unicast IPv4 manager
  and one local advertised address. The regular owned X server still binds
  loopback, and the XDMCP candidate checks the reserved endpoint and process
  owner before accepting readiness. The app describes listener readiness
  while manager authentication is pending. Both application modes, using
  numeric and hostname entry points, reached the existing fail-closed
  artifact gate in the locked Windows XDMCP replay (19 passed).
- The updated Windows runtime patch SHA-256 is
  `2199f0fc0751205d97f903bd2b0687f181e3337594837900c20fb00a8c577394`.
  The candidate executable SHA-256 is
  `d9466511531f881de451c4715a575935d58024b83ddb6823bc54b64a6a450bea`
  and its inventory SHA-256 is
  `22a4c1195d886eda5ebd3a8a3b50ca61b1e2240647d99c0e3660b9e24651a0fa`.
  In addition to both independent native wire cases, the Rust provider
  fixture on `127.0.0.2` completed Direct REQUEST, ACCEPT and MANAGE,
  authenticated with the manager's cookie and proved exact listener
  ownership and cleanup. This is a second loopback address, not an
  observation of physical adapter or routed-network behavior.
- The same inventory passed the embedded SSH case and a separate installed
  OpenSSH case through ConPTY, with Git for Windows' OpenSSH 9.9p2
  (executable SHA-256
  `50f2ed2c4eacea177dc208bbb70679307f5d5fcace69c2c74703da41c1b92ad9`).
  The test used a staged xauth 1.0.10, isolated SSH config and authority,
  strict host-key verification and a private fixture peer. The system
  Win32-OpenSSH 9.5p2 and the publisher's isolated 10.4p1 preview both
  reported no xauth data despite a direct xauth lookup succeeding. Neither
  passed authenticated forwarding in this fixture.
- The Windows XDMCP profile now accepts a single keyboard layout from the
  verified private XKB catalog and a numbered Windows monitor in fullscreen
  mode. The owned Direct fixture selected German, observed `z` at the
  X keycode for the top-row Y/Z key, and verified the X screen dimensions
  matched the first monitor's Win32 work area. The fullscreen path selects
  GDI and does not request a hardware display mode change. Cross-monitor
  focus, hotplug and native presentation still need the corresponding
  hardware and an unlocked desktop.
- `cargo +1.96.1 test --locked --test windows_conpty` passed both tests.
  The native child wrote the main console, entered the alternate buffer,
  accepted input and received a resize from 80 × 25 to 100 × 32. TermCraft's
  terminal parser consumed the real ConPTY output and observed the original
  primary screen after the alternate buffer closed.
- Windows VNC scope is GDI capture of the active primary desktop.
  `cargo +1.96.1 test --locked --lib services::vnc -- --test-threads=1`
  reported 20 passed and 4 ignored. The opt-in native inactive-desktop test
  passed separately, confirming the provider denied startup, capture and
  control on a private desktop that never received foreground input.
  The new authenticated one-pixel loopback probe compiled. It and the
  previous native greeting probe stopped before opening a listener because
  the host reported the interactive Windows session as disconnected. A
  disposable unlocked session is needed for the positive pixel/control
  replay. Windows providers for an individual window, a secondary monitor
  and a virtual display are still absent.

## Native build

The branch started from `98b213d`. This run used Windows 11 Pro x64 build
26300, Visual Studio Build Tools 2022 17.14.31, MSVC 14.44.35207, Windows SDK
10.0.26100.0, Rust/Cargo 1.96.1 (`x86_64-pc-windows-msvc`), and Python 3.11.9.
Cargo used two jobs and the GUI and RDP helper retained independent locks.

| Input or artifact | SHA-256 |
| --- | --- |
| GUI `Cargo.lock` | `14bd7db3fee1246d461898ab4a3d3fd68775bb0669b50aa8c2a9a8435081758a` |
| RDP helper `Cargo.lock` | `ca94e4a19abf7abaf809f319e8931d3c70eb95520bac8efa9afe7e4e1f014f07` |
| Package source snapshot | `4be740728ad76d2183f0cf6ce7aefd64cc9c73f0eb63c32680be2d398cab62df` |
| Packaged GUI | `a81d7871745f0e321d3b0ca929d9daec81766c54e6950004e5b1e84d86840b05` |
| Packaged RDP helper | `859860b28b8c5fad7c167088362d571343874feb88be3949cad82d1cc96249d9` |
| Unsigned Windows x64 ZIP | `353b7ad19227475388c55200bc46b5bcd0e03ebdc8d326d842e01400cdbb62c7` |

The package manifest records 990 payload hashes, the full frozen source file
inventory, native DLL imports and Rust versions. The frozen snapshot was
captured during development. Later branch changes have separate test evidence.

A second locked release build used code revision
`9525814498567d53b1939b5472b58adfbd5a98a1` and the working packaging guide.
Its exact package source snapshot was
`71e918eb77f23e3d00ba586e3f0ef00e79490adc8cf41bb8c8d7196708a9956a`.
The 990-file ZIP passed the same relocated startup probe.

| Second build artifact | SHA-256 |
| --- | --- |
| Packaged GUI | `f4afa8777ad6163aca3eaf8db5dafc7a7931b3722a7027ac50f995b47cc961ae` |
| Packaged RDP helper | `b7be54b2c03eeab5a96927eccd5d46ffd6f74ed7d23bfe96dae24175a978688e` |
| Unsigned Windows x64 ZIP | `96f650ab028099abe1100b11318fc8475620098b97dd65671a5b00f2ff54e52d` |

The release candidate for code revision
`2b88d9c46ab5a8c6bcea8dfa7758cdbb08b20e6b` passed relocated startup and
installed GUI startup, both with private workspaces. The package retains
990 payload hashes and the independently locked helper.

| Current release input or artifact | SHA-256 |
| --- | --- |
| Frozen package inputs | `2c586e943745695655bf5e38ccab85136beaa8d78c072fa04b66f30e4f0e7282` |
| Packaged GUI | `42db176b2656296c55ce6dd4eb8c111f50ff7983d58aef386722c6ee8cf9c2d3` |
| Packaged RDP helper | `fb78b95d3928b70a82d5f1f2d2d9c2d8d804394607d35b45a2d2d7f9409cdc0b` |
| Unsigned Windows x64 ZIP | `334e9b9ce06b6de3b58160c8da3233d4d9206abfe99896d4787a02ada4171d44` |

The follow-up package includes the reviewed Windows browser viewport and XKB
changes from `5069518`. It was built with
`python scripts/package-release.py --platform windows --build-root target/windows-qualification/build --output target/windows-qualification/5069518`
using Python 3.11.9 with Pillow 10.4.0 and Rust 1.96.1 in the x64 Developer
PowerShell. The manifest lists 990 files.

| Follow-up package input or artifact | SHA-256 |
| --- | --- |
| Frozen package inputs | `3d668b25964d7cba9b02391ec5a5394e3f99220482036e7fb89b0707a7ac21a6` |
| Packaged GUI | `1abefd58ae9ba6fbf66c62d58cafca38544ce7abd56b2a5021ca1117ecc5da83` |
| Packaged RDP helper | `ebf9fe64d792345d31e089f7370e3abbb6826702622704883d863f62c20840a1` |
| Unsigned Windows x64 ZIP | `09d30d59b2d9148cf6fb26f95703671b953a34bfb85e080700f5e3d9035cae37` |

The relocated ZIP passed the GUI screenshot and live RDP helper identity
probe. Its exact extracted RDP helper passed 10 software endpoint tests.
The current Windows account then exercised installed startup, shortcut
targets, foreign-file refusal, preserved workspace and encrypted vault, and
uninstall using the `2b88d9c` and `5069518` executables.
Replaying the installer with the same archives confirmed that duplicate
installation and uninstall over modified or extra files were refused.
After the original version was removed, a fresh install of the same
version matched its original executable digest and removed cleanly.

The local unsigned package rebuilt at `57c122a` declares the native
OpenGL 2.0-or-later GUI requirement and retains the 990-file layout.
The relocated GUI probe and all 10 RDP software-endpoint cases passed with
the exact extracted helper. The build command was the same as above, with
output `target/windows-qualification/57c122a`.
The installed `5069518` and `57c122a` GUI pair passed the expanded
same-account lifecycle. The synthetic vault hash matched its original
bytes and it unlocked after both installs and the reinstall were removed.

| Latest local package input or artifact | SHA-256 |
| --- | --- |
| Frozen package inputs | `2037fccdcd444d6998f9b78c831a244154574f8383298612a86d8ce002a662e0` |
| Packaged GUI | `de7c1fc4bdc9f6ee07dd1e0f71d53d6b1ba7c6233dde574881583bc19ef55299` |
| Packaged RDP helper | `ab6e6e704b226b79ef8d5c149dc9127b651550ecd4b0db79164f73654d775e8a` |
| Unsigned Windows x64 ZIP | `45f5f50404d09476cc4ba811011022e1c59675efe1d41cd0aa342f3af753978a` |

## Verified locally

- The first native helper run had 31 passes and four failures. Commit
  `31c8892` repaired handle-relative rename and corrected two test fixtures.
  The subsequent locked build ran 35 native helper tests with zero failures.
  The PC/SC test target ran zero tests on this configuration.
- The Windows archive suite ran 41 cases, with 18 passes and 23 conditional
  skips. It covers portable ZIP contents, pinned bytes in Windows checkouts,
  the project license and privacy policy, archive staging and isolated Cargo
  metadata resolution.
- The release ZIP was extracted under a path containing spaces. With an empty
  PATH and a private workspace, the GUI emitted a nonblank 1360×880 screenshot,
  the helper produced its version 1 handshake and a loopback fixture received
  initial RDP negotiation. The synthetic workspace session and preferences
  survived startup. A later replay observed the Windows child image via
  `Win32_Process` while the negotiation socket was open and confirmed the
  adjacent helper path and exact manifest digest.
- The per-user installer ran two synthetic version labels from the release ZIP.
  Both payloads and Start menu targets matched their expected values. Uninstall
  refused a foreign file, and both versioned directories and shortcuts were
  removed after the file was taken out. This exercised side-by-side install
  and removal on the build host. Clean-account upgrades, activation registration
  and preservation of real pre-existing Harbor data still require coverage.
  The second build also opened the installed GUI and emitted a screenshot
  from a private workspace before both test installs were removed.
  A distinct-build replay used the `9525814` and `2b88d9c` ZIPs with two
  synthetic installer version labels. Both executables kept the same saved
  SSH session, Dark appearance and unrelated file intact through startup and
  uninstall. This used the current Windows account and side-by-side installer
  semantics.
  A subsequent replay used `2b88d9c` and `1666bdc` plus a vault generated by
  the application from synthetic data. The installed GUI pair preserved vault
  SHA-256 `b6b60ede114175b8b17b6a4b9b03f37381f67ef7474060d818203ec48c145441`.
  The retained vault unlocked after both uninstalls and returned the same entry.
  The repeatable commands are `cargo +1.96.1 test --locked --test windows_vault_fixture -- --ignored`
  with `TERMCRAFT_TEST_VAULT_PATH` set to the private fixture, and
  `tests/windows-package-lifecycle.ps1 -Archive <current> -PreviousArchive <older> -VaultFixture <fixture>`.
- Native tests passed for PowerShell through ConPTY with redirected parent
  stdio, Unicode input/output, resize and normal child exit (1), private
  workspace dispatch (4), VNC application protocol behavior (12), and desktop
  gateway routing and cancellation (10). These use controlled software peers.
- In the second packaged GUI, System used the current desktop appearance,
  explicit Light and Dark repainted the main window, and Ctrl+Shift+B hid
  the sidebar. Normal window close persisted Dark and the hidden sidebar in
  the private workspace. This used the current native desktop scale.
- Ten opt-in native RDP integration cases passed against the separately locked
  IronRDP software endpoint with OpenSSL fixture certificates. They cover
  rendering, resize, pointer, cursor, authentication/trust prompts, carrier
  identity and cancellation. The tests also passed with the exact extracted
  release helper whose hash is recorded above.
- Native scratch-registry integration tests passed, including ownership and
  uninstall-guard checks. They do not establish clean-account protocol
  activation or upgrade behavior for an actual installed version.
- Normal startup and exit of the exact packaged GUI preserved two deliberately
  corrupt inputs in a private workspace. Before and after SHA-256 matched
  `aa511d8895eaacbd6dbc23190b08965ce2eddee3429b3e26929b96af3572e296`
  for synthetic malformed `workspace.json`, and
  `da997eec34a3d467c1603a1b4fe208d3122e095980f4b76a7d42cb9b75e4dff1`
  for synthetic malformed `credentials.vault`. This checked byte preservation
  at startup, separately from vault unlock and recovery.
- The pinned VcXsrv source archive and patch matched SHA-256
  `a73b30db3eb6b60efd181a1cd7fa39bc6df8f0d4c973d6c8416a856b0bf55b28`
  and `d647860419fca13a17cb2765e8d30a1c521016d66969f8fb297b6f46e3c4f8bb`.
  The Windows prep run applied the patch and emitted inventory SHA-256
  `63a21913d3f78dbc594ceab38475e9c6d046c71d94b43f6168da33c97985eaa6`.
  Native loopback XDMCP discovery tests passed independently of an X engine.

## Private Windows X build follow-up

The source archive stayed at `a73b30db3eb6b60efd181a1cd7fa39bc6df8f0d4c973d6c8416a856b0bf55b28`.
The revised patch SHA-256 is
`84338e9b4d583735a8591dc2bacf4eb021d355bac37f03cf11a68873a3dd8c4d`.
Fresh offline preparation with Git for Windows' GNU patch produced inventory
SHA-256 `d3eecec33cdc95a7467889d9dd3daa990d20311e2dcdf0c4de16e68376949a51`.
The compiler exposed an upstream `#undef` breaking expansion of
`SO_EXCLUSIVEADDRUSE` in the Windows SDK. The revised patch retains the
header definition and confines the reuse branch to other platforms.

The native recipe in [the build guide](../../../packaging/x11/native/README.md)
completed with MSVC 14.44.35207, SDK 10.0.26100.0 and the recorded Windows host.
Local tools included Jom 1.1.4, NASM 2.16.03, Strawberry Perl 5.40.2.2,
WinFlexBison 2.5.25, GNU gperf 3.1, Python 3.11.9, lxml 6.1.3, Mako 1.4.3,
PyYAML 6.0.4, Meson 1.12.1 and Ninja 1.13.2. The Cygwin installer invocation
was blocked by execution policy. The native build used independent portable
tools and Git Bash for the two generated-data recipes.

`stage-private-windows.ps1` assembled 5,138 files (84,815,893 bytes) in the
review tree. The private executable SHA-256 is
`0dae642a7b58d200f205687dd516c819e9b59845bbdf74c62a47d1384216d6cd`
and the staging inventory SHA-256 is
`577f463838f5cf78e2d466c325a9135553399648f3253edc08ee5b27547478dc`.
MSVC `dumpbin /dependents` resolved 33 import entries adjacent to the engine,
113 via Windows API sets and 46 in Windows System32. The local candidate
inventory and PE report are under `target/windows-x11-source`. Native XKB
data came from xkeyboard-config 2.44. The corresponding source is retained
under the same directory, with upstream license copies in the source tree
and existing repository native license inventory.

A direct startup probe against the private executable refused a loopback
port held by an independent listener. It observed precisely one owned
`127.0.0.1` listener on the chosen port, rejected anonymous and incorrect
cookies, accepted the matching cookie and observed listener removal on exit.
The Rust qualification case
`cargo +1.96.1 test --locked --lib privately_built_engine_owns_listener_and_stops_cleanly -- --ignored`
passed once. It exercised the real hash-verified snapshot, provider-owned job,
IP Helper PID checks, startup authentication and stop with runtime directory
removal. Review found that the first candidate could invoke the Windows
Command Processor for XKB. A follow-up patch starts the adjacent compiler
directly using `CreateProcessA` with an explicit executable. The apps and
release bundles retain the compiled approval gate. Full extension workflows
and redistribution review remain open.

The shell-free revision was prepared again from the pinned archive. Patch
SHA-256 is `5a474ac9381b2dd6b9ff887bb133fcd58713d47efc13e7b0040fa6b669de202b`.
Prepared input inventory SHA-256 is
`4545c5bd8d717dd77b5cbeb804b8b8314a3a9d2af2eac293f7c06d56dc0450ba`.
The rebuilt executable is
`d01963da0e08a550caa0e47b622e57ae5e6bb33a63fc0c7390777aa0dcd08880`,
staged inventory is
`a6f3109e41ca436c3a579b338c25b3a18b5eea985ef26215eaa1570e1cf9e847`,
and PE report is
`16675973bfe71efda0ed78def164688c8b520d8cf2880446ac2f6d9b58758113`.
These files are under `target/windows-x11-source`. The 5,138-file payload
still resolves the same import categories. Native owned-provider startup,
authentication and shutdown passed after removal of COMSPEC.
The same provider test reused the independent loopback SSH fixture. It
rejected an invalid forwarded cookie, opened a valid SSH X11 channel,
created and mapped a real X window in the private engine and observed the
viewable window through the forwarded connection. The engine and listener
were then removed through owned shutdown.

The same shell-free candidate passed the standalone Windows XDMCP peer test
`python tests/windows_xdmcp_candidate.py --candidate <staged tree> --inventory <staging report> --evidence <new directory>`.
An independent loopback UDP peer received DirectQuery, sent WILLING, received
REQUEST, assigned a synthetic X cookie and observed MANAGE. The actual
Windows X listener denied anonymous setup and accepted the assigned cookie.
For IndirectQuery the peer verified the actual request and returned WILLING.
The probe does not complete an indirect chooser session, launch a real
display manager or enable the Windows application provider.

## Windows browser compatibility follow-up

`cargo +1.96.1 test --locked --test browser_session -- --include-ignored --test-threads=1`
passed all 5 tests with an isolated Chrome for Testing 158.0.8127.88 win64
installation and local loopback HTTP, proxy and HTTPS fixtures. The official
test archive observed in this run had SHA-256
`fda73ca9ffc7f02c5c85165cb472152540a76a7cecd8ec11314ea781e4454109`.
The test installation was removed from the Chromium known folder after the
run. The resolver recognized the official test build through the installed
compatibility path.

Three fixture tests originally stopped because the private bootstrap tried
Windows known-folder resolution after substituting the isolated profile
environment. The parent now resolves the installed engine before launching
the helper, and the authenticated helper rechecks the resolved image. The
remaining failure measured a 500-pixel capture for a 300-pixel pane. Windows
uses explicit page metrics and captures the live viewport. Finishing an active frame
before resizing prevents an older Chromium capture from restoring stale page
geometry. The passing replay covered repeated 300×200 and 460×320 panes,
pointer clicks at responsive coordinates, wheel scrolling with matching preview
and below-the-fold pointer input, Unicode form text, navigation,
proxy and site authentication, and untrusted TLS rejection. An independent
Windows token observer identified the fixture executable and queried live
browser and renderer tokens. The browser broker ran at medium integrity
(`0x2000`). The observed renderer processes ran with restricted tokens
at untrusted integrity (`0x0000`). No fixture process was running in
AppContainer, and the per-session profile directories were gone after exit.
This records the observed installed compatibility sandbox. A Windows
redistributable browser payload still needs a separate license/resource
audit, artifact pins, and the same startup and sandbox probes.

An inspected Windows x64 Ungoogled Chromium 158.0.8127.36-1.1 archive
matched the publisher's SHA-256
`bc78b9590881f8ca99aca29288d1b347ffdd689ca8a57384bb0bc113ef687b6b`.
The recipe commit was verified on GitHub. The ZIP had no detached signature,
artifact attestation or included component notices. Windows identified
`chrome.exe` and 13 other PE files as unsigned. This was a static provenance
assessment. [Publisher release](https://github.com/ungoogled-software/ungoogled-chromium-windows/releases/tag/158.0.8127.36-1.1).

## Pageant

The standalone Windows x64 Pageant 0.85 binary matched the publisher's
SHA-256 `3bd76b57fcd09acc948268c753ea2077b0c557ca435323301a9b572661f98ecd`
and Windows Authenticode validated the Simon Tatham signature. The command
`cargo +1.96.1 test --locked --test windows_pageant -- --ignored` passed
one test. It checked ownership of the fixture window by the launched PID,
then added, listed and removed a synthetic key in an initially empty agent.
A follow-up replay verified a Pageant signature over synthetic SSH signing
data and proved the same connection refused signing after key removal. The
fixture process and window were removed after cleanup. Real-account agent
forwarding remains open.

## Native desktop capture and hosted Windows build

The real Windows VNC view-only provider passed
`cargo +1.96.1 test --locked --lib windows_native_view_only_capture_starts_and_releases_listener -- --ignored`.
A native primary-desktop frame validated before publishing the loopback
listener. A local client read the RFB 3.8 greeting, disconnected before
authentication, and observed listener removal after stopping the managed
service. The test retained no screenshots and sent no desktop pixels,
clipboard data or input to the connection.

[The clean Windows 2025 runner](https://github.com/asad-albadi/TermCraft/actions/runs/45692468865/job/139481228493)
passed the package contract suite and built the unsigned ZIP
`606ff9ab626e1e6bb4018c2f32cb2c3759670638e961d5098baa48d229a574f3`.
Packaged command-line help and the adjacent RDP helper wire handshake passed
there. The very next GUI step returned
`OpenGL(PainterError("egui_glow requires opengl 2.0+. "))`. The installed-GUI
test was skipped after that failure. The same downloaded ZIP passed relocated
native GUI startup and the exact RDP child identity check on the supported
local Windows desktop. It also passed the installed GUI and exact-reinstall
test beside the local package on that desktop. This establishes a clean build, standalone helper
startup and supported-host GUI behavior. Clean-user GUI, genuine
release-to-release upgrade and activation still require a Windows desktop
with an OpenGL 2.0 or later context.

## Outstanding evidence

- The first executable root run reported 557 passes, 46 failures and 9 ignored
  checks. Revision 9525814 reported 597 passes, 7 failures and 9 ignored checks.
  The 2b88d9c locked root run reported 606 passes, zero failures and 19 ignored
  checks. Ten of those ignored cases are the independently executed native RDP
  software fixture. Windows system-local Cron scheduling was tested using the
  host timezone. The fixed New York DST subprocess fixture still requires a host
  where that timezone can be selected without changing the user's settings.
- The follow-up `5069518` Windows root run reported 606 passes, zero failures
  and 22 ignored. The independent locked RDP helper replay reported 35
  passing tests. Windows browser (5), RDP software endpoint (10), owned X
  (1), Pageant (1), synthetic vault (1) and two terminal benchmarks were
  executed separately. The terminal model processed 360,000 input lines and
  retained 113,751,152 bytes within its 128 MiB budget. Native GPU frame timing
  was outside the CPU renderer benchmark.
- The final `57c122a` locked Windows root suite reported 606 passes, zero
  failures and 23 ignored. The extra opt-in test is the native VNC capture
  case, which passed separately on the supported desktop.
- The private VcXsrv build, owned start/stop and controlled embedded SSH
  forwarding passed. The runtime approval pin remains unset while MSVC
  redistribution notices, installed OpenSSH forwarding, Direct and Indirect
  desktop workflows, clipboard, keyboard and graphics qualification are open.
- Terminal GUI input, full-screen programs, detached workflows, themes and the
  100/150/200 percent scaling matrix require their own desktop observations.
- Full RDP sessions and redirection need a designated endpoint. Windows
  enumerated a USB serial port and a software PDF queue. Neither was exercised
  as a physical fixture. Smartcard, physical printer, connected serial/network
  peers and the original Personal 26.6 installation are outstanding.
- Windows bundled Chromium needs an audited Windows payload and resource
  pins. Native capture/control, Pageant-backed SSH, and vault recovery and
  permission workflows retain their separate qualification gates.
- The earlier BusyBox-w32 archive mismatch was resolved in the continuation
  above. Both Python 3.10.11 and 3.11.9 independently assembled the approved
  archive from the exact pinned upstream inputs.
- The publisher lists the portable Home Edition 26.6 and distinguishes Home
  from Professional Customizer in its [edition comparison](https://mobaxterm.mobatek.net/download.html)
  and [version listing](https://mobaxterm.mobatek.net/download-home-edition.html).
  This is publisher documentation. The supplied original executable was
  unavailable for this local run. The publisher attaches license acceptance
  to the download, so an authorized copy is needed for the original-product
  workflow replay.
- The local process used an unelevated Windows account and Windows Sandbox
  was absent. The clean hosted run and local development version-upgrade
  evidence above cover different scenarios. The repository had no published
  release when queried. A long-lived user upgrading from a previously
  released package and a manual desktop first-login replay remain open.
- [Hosted Linux CI](https://github.com/asad-albadi/TermCraft/actions/runs/45692458444)
  reproduced the existing redirected-drive allocation cleanup failure
  tracked separately in [issue #3](https://github.com/asad-albadi/TermCraft/issues/3).
  The failure was in the Linux-specific helper test. The native Windows
  helper replay independently passed 35 tests.
- The Linux release job in the same hosted release matrix also failed its
  package contract because the pinned Linux iperf2 binary requires
  `GLIBC_2.43` and the Ubuntu 24.04 runner did not provide it. This was
  observed in the Linux job before archive publication.
- The continuation's [Linux release job](https://github.com/asad-albadi/TermCraft/actions/runs/45747315026/job/139666403527)
  reproduced the same iperf2 ABI failure. The [macOS arm64 job](https://github.com/asad-albadi/TermCraft/actions/runs/45747315026/job/139666404631)
  failed with four Rust mode-type mismatches in Unix remote-file transfer.
  Both source files and the dependency lock match main `cf7ee4d`
  byte-for-byte. [CI on main](https://github.com/asad-albadi/TermCraft/actions/runs/45745650311)
  and [CI on the branch](https://github.com/asad-albadi/TermCraft/actions/runs/45747710710)
  independently reproduced the Linux allocation cleanup failure.
