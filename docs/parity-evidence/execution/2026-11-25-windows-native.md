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
  survived startup. The Windows startup probe does not identify the child
  image separately from verification of the archive and extracted helper.
- The per-user installer ran two synthetic version labels from the release ZIP.
  Both payloads and Start menu targets matched their expected values. Uninstall
  refused a foreign file, and both versioned directories and shortcuts were
  removed after the file was taken out. This exercised side-by-side install
  and removal on the build host. Clean-account upgrades, activation registration
  and preservation of real pre-existing Harbor data still require coverage.
- Native tests passed for PowerShell through ConPTY with redirected parent
  stdio, Unicode input/output, resize and normal child exit (1), private
  workspace dispatch (4), VNC application protocol behavior (12), and desktop
  gateway routing and cancellation (10). These use controlled software peers.
- Ten opt-in native RDP integration cases passed against the separately locked
  IronRDP software endpoint with OpenSSL fixture certificates. They cover
  rendering, resize, pointer, cursor, authentication/trust prompts, carrier
  identity and cancellation. The run used the Windows MSVC debug helper and
  the Windows build of the fixture.
- The pinned VcXsrv source archive and patch matched SHA-256
  `a73b30db3eb6b60efd181a1cd7fa39bc6df8f0d4c973d6c8416a856b0bf55b28`
  and `d647860419fca13a17cb2765e8d30a1c521016d66969f8fb297b6f46e3c4f8bb`.
  The Windows prep run applied the patch and emitted inventory SHA-256
  `63a21913d3f78dbc594ceab38475e9c6d046c71d94b43f6168da33c97985eaa6`.
  Native loopback XDMCP discovery tests passed independently of an X engine.

## Outstanding evidence

- The full Windows regression gate remains open. The first executable root run
  reported 557 passes, 47 failures and 9 ignored checks. The 9525814 run
  reported 597 passes, 7 failures and 9 ignored checks. Failures were in Cron
  system-local DST, FTPS protected download shutdown, three FTP workspace
  workflows, unavailable Windows packaged Iperf and unused-port classification.
- The native VcXsrv build still requires the upstream Cygwin tool set and
  Strawberry Perl, staging the matching DLLs/fonts/XKB, MSVC redistribution
  notices, and native engine qualification. The build host has MSVC and its SDK.
  The runtime approval pin is unset. X startup, SSH X forwarding, Direct and
  Indirect desktop workflows, clipboard, keyboard and graphics are open.
- Terminal GUI input, full-screen programs, detached workflows, themes and the
  100/150/200 percent scaling matrix require their own desktop observations.
- Full RDP sessions and redirection need a designated endpoint. Windows
  enumerated a USB serial port and a software PDF queue. Neither was exercised
  as a physical fixture. Smartcard, physical printer, connected serial/network
  peers and the original Personal 26.6 installation are outstanding.
- Windows bundled Chromium needs an audited native payload and resource pins.
  Native capture/control, Pageant, vault recovery and permission workflows
  retain their separate qualification gates.
