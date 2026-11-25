# Windows x64 qualification for issue #1

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
- The optional BusyBox-w32 inputs passed the upstream binary, source and license
  checksum checks. Python 3.10.11 and 3.11.9 both produced assembly SHA-256
  `4649824ec37541b1939b0c18805b6475adba268bbbd743525413e033803cf081`
  against the existing archive pin
  `778b85b95d82d510e77703306e2a9d80fa11af43d1048b96338ff351eab5b7b2`.
  Staging rejected the result. The current release omits this optional payload.
  The exact upstream UTF-8 `ash.exe` independently read a scratch file through
  a Windows drive path containing spaces. The packaged Unix installer probe
  remains gated by the archive fingerprint.
- The publisher lists the portable Home Edition 26.6 and distinguishes Home
  from Professional Customizer in its [edition comparison](https://mobaxterm.mobatek.net/download.html)
  and [version listing](https://mobaxterm.mobatek.net/download-home-edition.html).
  This is publisher documentation. The supplied original executable was
  unavailable for this local run. The publisher attaches license acceptance
  to the download, so an authorized copy is needed for the original-product
  workflow replay.
- The local process used an unelevated Windows account and Windows Sandbox
  was absent. Current-account package observations do not establish
  first-login, fresh-user or real release-to-release installation behavior.
- [Hosted Linux CI](https://github.com/asad-albadi/TermCraft/actions/runs/45692458444)
  reproduced the existing redirected-drive allocation cleanup failure
  tracked separately in [issue #3](https://github.com/asad-albadi/TermCraft/issues/3).
  The failure was in the Linux-specific helper test. The native Windows
  helper replay independently passed 35 tests.
- The Linux release job in the same hosted release matrix also failed its
  package contract because the pinned Linux iperf2 binary requires
  `GLIBC_2.43` and the Ubuntu 24.04 runner did not provide it. This was
  observed in the Linux job before archive publication.
