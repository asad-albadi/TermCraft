# D02 native smartcard independent review

Verdict: ACCEPTED for the supplied source handoff. No important correctness, security, or specification findings remain in this exact candidate. This accepts source integration; it does not qualify real native runtime/hardware behavior.

## Exact inputs and integrity

Reviewed the 19 paths in ../d02-native-smartcard/integration-owned-manifest.json, frozen baseline, candidate, source_inputs, payload and integration.patch. Independently reconstructed the frozen baseline, applied the exact patch, and reproduced every payload SHA256. Verified every owned source_inputs file against the candidate SHA256. The helper manifest's payload retains the baseline release identity; the candidate-only identity changes are documented and excluded. Independent checks confirmed pcsc.rs, session.rs, ndr.rs and the entire Linux worker module are byte-identical to frozen baseline. Details: integrity-review.json; patch-verify/ contains the independently patched result.

## Focused review

- Native ABI: checked Apple official fixed-width int32 handles/results, uint32 arguments, packed reader state and PCI, ATR33 and status flags against the actual published headers. Windows uses ULONG_PTR handles, DWORD/LONG32, natural reader layout/ATR36, UTF-16 W APIs and system ABI. Checked library object retention during every function pointer call and partial-symbol failure cleanup. Readers-list lengths, UTF-16 conversion, status ATR bounds and transmit output/PCI bounds are guarded.
- Library loading: Windows derives its absolute System32 WinSCard.dll path from GetSystemDirectoryW and restricts dependency search; macOS uses the protected absolute framework. None returns before child creation or API/library access. Selected-reader/context/card/APDU policy stays in unchanged Session/NDR, including opaque remote handles and zeroized owned buffers.
- Framing/ownership: 4-byte lengths are rejected before payload allocation; full MAX_PACKET survives fragmentary reads/writes. Broker and child queues are bounded. Driver calls run in the child, while parent threads move bytes. Child death closes pipe endpoints before broker joins. Windows explicit HANDLE_LIST and atomic JOB_LIST occur before ResumeThread; job is private, kill-on-close and never inherited. macOS kills its isolated group before waiting, with the unreaped Child anchoring group identity; validated child group and independent watchdog cover blocked native calls and parent death. No global PC/SC service signaling exists.
- Launch identity: no user/environment-supplied alternate executable path was introduced. Followed existing release launch ownership through src/desktop/mod.rs and src/runtime/manifest.rs: macOS launches verified bytes in a private snapshot directory retained for the RDP session; Windows verifies/locks the image through initial process creation, then the running executable remains the image being relaunched. Native current_exe resolves this owned helper. This review did not substitute an arbitrary fixture path for production source behavior. Native OS launch/replacement timing remains a native runtime qualification item, as already disclosed by the handoff.
- Command/environment: role argument is fixed and the corrected Unicode/Program Files quote regression passes. Native argv carries no reader/APDU; reader configuration and requests travel over inherited private pipes. macOS retains the existing loader-variable sanitizer; Windows uses a clean OS-derived SystemRoot environment. The shared main/capability/UI/Cargo hunks are scoped to smartcard enablement/API features and need ordinary union merging with the independent printer patch.

## Independent execution

All Cargo operations used --offline --locked -j 2. No lock generation was needed. Sources copied privately, touched after copy, identities changed only in private review inputs. Helper package 0.1.938 / termcraft-rdp-smartcard-review938; component d02-smartcard-review938-component 0.1.938 / d02-smartcard-review938-fixture. No app build or MAIN writes.

| Check | Independent result | Evidence |
|---|---|---|
| Existing Linux helper | 57 unit + 10 integration passed | helper-tests.log |
| macOS ABI simulation and real framed worker on Linux | 11 passed | mac-tests.log |
| Windows ABI simulation and real framed worker on Linux | 11 passed | win-tests.log |
| Windows component source, x86_64-pc-windows-msvc --tests | exit 0 | win-check.log |
| macOS component source, aarch64-apple-darwin --tests | exit 0 | mac-check.log |
| Exact patch/payload hashes and Linux byte identity | all matched | integrity-review.json |

The process tests exercised four-second readiness/native-call hang handling, drop/reap, parent-death watchdog and unrelated-child survival. These are controlled Linux executions of shared code and ABI callbacks, not Windows Job behavior, Darwin launch behavior, native SDK linking, PC/SC daemon access or reader/card qualification. No real hardware/daemon/system mutation was attempted. All owned Cargo/tool sessions returned exit 0 and all fixture processes were reaped; final process audit found no review938 process. Commands are in commands.txt.

## Primary references inspected independently

- [Apple pcsclite.h](https://raw.githubusercontent.com/apple-oss-distributions/SmartCardServices/main/src/PCSC/pcsclite.h)
- [Apple winscard.h](https://raw.githubusercontent.com/apple-oss-distributions/SmartCardServices/main/src/PCSC/winscard.h)
- [Apple wintypes.h](https://raw.githubusercontent.com/apple-oss-distributions/SmartCardServices/main/src/PCSC/wintypes.h)
- [Microsoft winscard.h](https://raw.githubusercontent.com/microsoft/win32metadata/main/generation/WinSDK/RecompiledIdlHeaders/um/winscard.h)
- [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [CreateFileW sharing rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
