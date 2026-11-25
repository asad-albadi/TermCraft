# D02 native smartcard source handoff

Implemented Windows and macOS smartcard provider source in a private candidate. No MAIN writes, physical reader/card access, real PC/SC daemon access, or native runtime success claims. Full app/parent integration gate remains root-owned.

## Source behavior

- Windows loads an absolute OS-derived System32\WinSCard.dll path with system-only dependency search. Native calls use the system ABI, UTF-16 W entry points, ULONG_PTR handles, fixed DWORD/LONG, natural-layout reader states/ATR36 and PCI8. Windows status enums pass through; Apple legacy unsupported-feature conversion never applies on Windows.
- macOS loads only the absolute protected PCSC framework. Its native adapter uses Apple int32 handles/results, uint32 scalar parameters, packed(1) reader state/PCI and ATR33. It converts Apple status bits to the remote Windows state enumeration.
- Both adapters expose the same Api to unchanged Session/NDR. Remote handles remain opaque; reader selection, card/context binding, context/card/watch caps, watch/cancel/release semantics, APDU/output bounds and zeroized owned buffers remain shared.
- Both new platforms use 4-byte length frames across anonymous inherited pipes. Full MAX_PACKET is supported without assuming any Darwin datagram capacity. Bounds precede allocation; partial reads/writes are handled, parent queues are bounded (16 outgoing/17 incoming), and duplicate completion IDs/backpressure are preserved.
- Native driver calls run only inside the owned helper child. Two parent broker threads move framed bytes. Windows uses STARTUPINFOEX HANDLE_LIST plus atomic JOB_LIST membership, a non-inherited kill-on-close Job, suspended creation and resume after ownership. This requires Windows 10+, compatible with the app baseline; startup fails closed without the API. macOS owns an isolated process group through an unreaped Child, kills before waiting (PID reuse cannot retarget the group), and the child validates group identity before starting a parent-death watchdog. The watchdog can terminate that live owned group during a hung vendor call. No provider ever signals pcscd or another global service.
- Helper --pcsc-worker dispatch is wired for all three platforms. Capability and reader-add UI gates now include Windows/macOS after provider wiring. Saved JSON is unchanged; None returns before process creation or library loading.
- Linux pcsc.rs, session.rs and ndr.rs are byte-identical to baseline. Its existing SOCK_SEQPACKET worker behavior is unchanged.

## Evidence

| Check | Result |
| --- | --- |
| Existing Linux helper unit tests | 57 passed |
| Linux real private fake-library fixture integration | original 8 preserved + 2 framing tests = 10 passed |
| macOS ABI mock + actual common framed worker on Linux | 11 passed |
| Windows ABI mock + actual common framed worker on Linux | 11 passed |
| Native Windows/MSVC component source check | passed for x86_64-pc-windows-msvc, --tests |
| Native Darwin component source check | passed for aarch64-apple-darwin, --tests |
| Component formatting | passed |
| Exact payload patch | clean application to frozen baseline; payload hashes verified |

Controlled callback tests exercise exact ABI layouts, Unicode reader matching/W conversion, native PCI sizes, state and error mappings and APDU replies. Common transport tests exchange real framed requests through owned fake children and unchanged Session: Unicode list/connect/status/APDU, unselected-reader rejection, cross-context handle rejection, malformed APDU rejection, watch cancellation, stale handles, and None no-spawn. They also test full MAX_PACKET with fragmented reads, malformed/truncated/oversize frames, four-second readiness/native-call timeouts, drop/reap with waitpid ECHILD, and survival of an unrelated child.

Abrupt parent death during a deliberately hung native establish is audited in a disposable Linux subreaper process. It checks the exact shared POSIX watchdog logic and reaps the adopted child; it is not a macOS runtime qualification. Windows Job/handle inheritance and system DLL loading are source/type evidence only.

Actual behavior RED/GREEN: first-pass Windows command construction had a literal backslash before its opening/closing quote. Extracting that logic into the shared UTF-16 command builder produced the real failing Unicode/Program Files path regression in red-command-quoting.log (1 failed), with red-process-command.rs retained. The corrected quote-only implementation passes green-command-quoting.log. Earlier scaffold/fixture stdout failures are not represented as native platform regressions or runtime qualification.

## Integration

Apply integration.patch or merge payload using integration-owned-manifest.json. Each entry gives exact baseline/candidate/payload SHA256. Preserve the printer worker's independent options/UI/Cargo hunks; the only options/UI changes here are smartcard capability and Add smartcard reader gates. Engine Cargo additions are only the required Windows native API features. Merge feature lists as a union if other device work touches that line.

Private manifests identify app 0.1.935 / termcraft-native-smartcard935 and helper 0.1.935 / termcraft-rdp-smartcard935. Identity-only app/helper lock, manifest and Linux fixture binary-name changes are excluded from integration payload. The helper payload manifest retains baseline version/default binary naming. New test component has its own canonical lock and private identity. Its two fixture features are mutually exclusive Linux-hosted ABI simulations; native target checks use neither feature.

source_inputs contains the complete candidate source/test/config inputs (target and dist build artifacts excluded). tests-snapshot separately captures every candidate test input. No source depended on an unstaged target-relative file or empty directory. Native source qualification remains pending for real Windows/macOS SDK linking, launch/inheritance/runtime timing, service/reader/card behavior, and vendor driver testing. No such access was attempted here.

All cargo checks/tests/builds use --offline --locked -j 2. Root explicitly allowed one canonical initialization of the new component lock with CARGO_BUILD_JOBS=2 cargo generate-lockfile --offline. Its hash and the ruling are in private-identity.json. All test subprocesses and tool sessions were reaped; process audit is in process-audit.json.
