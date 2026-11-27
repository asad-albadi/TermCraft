# Joint Windows and Apple Silicon integration

Progress remains 46/60 reviewed implementation checkpoints (76.7%). Native and original-workflow acceptance is still partial.

## Inputs and authorized scope

- Base: `cf7ee4d2f1373f18302a0f083bf3d4a3860b42f6`.
- Windows PR #6: `2dfe610046f679322b29e1e9e25544cabab2cfb8`.
- Apple Silicon PR #5: `2e24450bb7a30f1fa15d094bd949a8483f8dcd08`.
- User authorized joint review, shared CI fixes, conflict resolution and merge after verification. Issues #1/#2 retain hardware/manual checks; release-only gates belong to #3. Intel macOS and Android are excluded.

## Execution checklist

- [x] Fresh independent code review of Windows changes; no critical/important findings. Read-only, not a replay of native execution.
- [x] Fresh independent code review of Apple Silicon changes; no critical/important findings. Same limitation.
- [x] Resolve shared conflicts preserving both native implementations, evidence and approval gates. Independent review found a Windows ZIP publication regression; its regression test failed before the fix and passed after, with a fresh independent review.
- [ ] Diagnose and fix the inherited Linux RDP allocation-cleanup CI failure with regression evidence.
- [x] Rebuild and repin only the Iperf engines from tracked source on Debian 12; native packaging and real TCP/UDP checks pass on Debian 12 and Arch. This does not qualify other Arch optional runtimes on Ubuntu. Direct hosted Ubuntu verification remains pending.
- [ ] Run combined Linux regression/packaging checks and review integration changes.
- [ ] Push reviewed integration fixes, verify hosted checks and merge PRs #5/#6.
- [ ] Reconcile issues/checklists and proceed to release automation in #3.

Native agents reported Windows 619/0/36 ignored and Apple Silicon 922/0/17 ignored. These are separate platform runs, not fresh combined-candidate results. Private artifact contents are not independently qualified by this Linux integration review.

## Fresh integration checks

- Library suite: 316 passed, zero failed, 30 ignored. Rust 1.96.1, CPU 0. Initial unrestricted linker attempt crashed in LLVM; affinity-limited retry completed.
- Python packaging discovery: 53 tests, success with 20 skipped. Native-only skips are not qualification.
- macOS evidence/XQuartz contract discovery: 7 passed; release workflow contract: 1 passed.
- Full all-target Linux suite is running; results not yet claimed.
- Independent allocation review identified a concurrent local-writer truncation risk in the initial ext4 fix. A scoped write lease now protects ordinary concurrent openers; independent review accepted the guard with the kernel timeout limitation retained. Fix-branch tests passed 86 unit + 10 integration, and the combined helper is being tested separately.

The first local all-target build failed before execution with an out-of-range debug-info linker relocation. A fresh non-incremental build is running. No full-suite pass is claimed from that failed invocation. Shared fixes are pushed to Windows PR #6 at b91a350; hosted PR CI run 45919563373 is in progress. Linux CI now pins Rust 1.96.1 and reports its versions; the manual release builder uses the same verified SDK.

## Native regression gate follow-up

The second candidate (4ca75f9) includes the reviewed runner prerequisite and serial test repairs. Manual package run45923253954 is in progress; its macOS helper step passed. PR CI runs45923194474 (Windows branch) and45923239267 (combined branch) are in progress. The unavailable local non-incremental retry has no retained result after an environment refresh and is not counted as a pass.

A full Apple Silicon workspace job is now required in CI, with explicit arm64 identity, SDK14 deployment floor, Rust1.96.1 and prepared locked RDP fixture graphs. Hardware/runtime opt-ins remain separate. Push CI is restricted to main; PR/manual checks remain, and superseded runs are cancelled by PR/ref group. This removes duplicate push/PR builds without removing the native regression gates. The new configuration changes no application source; native execution is still required.
