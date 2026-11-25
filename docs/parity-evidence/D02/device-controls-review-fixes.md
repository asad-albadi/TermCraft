# D02 device controls FIX1 independent rereview — 2026-11-25

Disposition: **both prior Important findings are ADDRESSED** in the refreshed private candidate. No new Critical or Important defect was established in the fixes. This is acceptance of the scoped source increment for integration, not native runtime validation or D02 completion.

## Findings

1. **Asynchronous native count pointers — ADDRESSED.** `engines/rdp/src/serial_windows.rs:272`, `:279`, `:289` now pass `std::ptr::null_mut()` to ReadFile, WriteFile and DeviceIoControl. The unused dispatch-local count is gone. GetOverlappedResult retains its own local count storage for polling and waiting. This implements the previously requested asynchronous API contract; no memory-corruption demonstration is claimed.

2. **Actual terminal outcome during close — ADDRESSED.** `engines/rdp/src/serial_windows.rs:328`–`:350` adds explicit cancel-and-retire. The close path at `:427`–`:432` collects its response instead of preselecting CANCELLED. Cancellation is requested, GetOverlappedResult waits with `bWait = 1`, then the real success count or mapped failure enters `serial_completion::cancel_and_collect`. A successful write preserves its byte count, a read/control preserves its output, and the existing native mapper maps ERROR_OPERATION_ABORTED to cancellation while retaining other error mappings. `finished` is set after collection; the boxed OVERLAPPED, event, vectors and device handle remain owned throughout the wait. Drop remains a response-free cleanup path and does not free pending buffers on a timeout. The native calls still run on the device worker.

The new `engines/rdp/src/serial_completion.rs` is used by both regular completion and explicit close, so the tests exercise the actual production response encoder. Its sequence invokes cancellation before awaiting the terminal outcome, then encodes that outcome. The extracted helper does not simulate a Windows driver or establish native cancellation timing. No fix changed the regular-file mutation or Darwin allocation implementation.

## Independent evidence

- Refreshed patch SHA-256: `0f9475f526e7150e5d4f762a409ff59bb885d45433161f732f8c623c2000998c`.
- All **31 base** entries and **31 owned** entries matched their manifests, including expected absent files. Current-main-tree patch dry-run returned 0. The concurrent unrelated main-tree integrations did not invalidate the D02 bases.
- All seven production inputs in `component-production-input-sha256.json` matched both the handoff and actual private check inputs. The existing generated Drives file matched its recorded hash. Independent reconstruction from the handoff source and generator transformations reproduced `74920da79c7eba784bc3c8e3328d1b6f0a3e6886a4bcbea331f618449cc31f50`.
- Independently reran the exact production completion tests with `CARGO_BUILD_JOBS=2`, the existing shared target cache, and `cargo test --offline --locked --manifest-path engines/rdp/Cargo.toml --bin d02-device-private-rdp serial_completion::tests -- --nocapture`: **3 passed, 0 failed**, 44 filtered out. These cover successful-write count, real read/control output, abort status and a different terminal error. An initial invocation using a cold default target directory was stopped during dependency compilation, before tests ran; only the subsequent completed cached invocation is counted.
- Independently reran `CARGO_BUILD_JOBS=2 cargo check --offline --locked --manifest-path tests/rdp-native-component/Cargo.toml --target x86_64-pc-windows-msvc --tests`: **passed**, with the already documented unused metadata variable warning.
- Independently reran the same locked component command for `aarch64-apple-darwin`: **passed**.

The owner-recorded three behavioral RED failures, 16-test serial/completion GREEN run and formatting pass were inspected as supplied evidence, not independently rerun in full. The earlier 44-unit/8-PCSC-integration run predates FIX1 and is not current post-fix whole-helper evidence. The root coordinator still owns merged whole-helper/application verification.

## Remaining boundaries

Native Windows API execution, close races against a real driver, hardware restoration/unplug behavior and full Windows helper linking/packaging remain unrun. The full Windows helper's earlier upstream libz-sys/host-compiler failure is not replaced by component typechecks. An indefinitely unresponsive driver can still retain its worker/resources; no unsafe early-free workaround was introduced. Native file mutation/allocation fixtures remain compile-only. All other D02 gaps in the original review and handoff README remain open. No checklist count changes are authorized by this rereview.

No implementation edits, broad tests, physical-device access, printer/clipboard changes or network-device operations were performed. All review-started jobs are finished.
