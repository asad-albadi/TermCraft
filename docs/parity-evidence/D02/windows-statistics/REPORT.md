# D02 Windows per-open serial statistics candidate

Eight integration files change. Windows logical CREATE now reserves a monotonic handle ID, keeps it unpublished, and starts a read-only overlapped GET_STATS probe on its pinned native file. Successful 24-byte samples establish that handle’s immutable origin. Both ordinary and close-time GET_STATS completions subtract each of the six origin words modulo 2³². Failed, malformed, timed-out or unavailable probes leave the port operational and GET_STATS unsupported on that open. SET/CLEAR_STATS, name reopening and physical statistic resets are absent.

Microsoft documents all six native counters as cumulative since native open or last clear. The protocol maps GET_STATS to 0x001B008C. Cancellation does not itself discharge native ownership. [Native counter semantics](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ns-ntddser-_serialperf_stats) · [MS-RDPESP requests](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/824cc217-febd-4a2e-a95f-5b5bdc32848f) · [CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)

## Ownership and scope

The probe has a 100 ms deadline, polled by the worker at approximately 5 ms intervals while dispatch remains responsive. Holding the original CREATE until its one reply retains its existing ordinary credit. Unpublished opens reject data, close and control requests and duplicate opens. Successful CREATE replies retain admission order. The worker now polls completions before selecting each next request, including after a printer request.

Each actual probe has a dedicated bounded owner. Before spawning or issuing I/O it reserves one process-wide permit (16 total) and one shared-binding permit that persists across certificate attempts. The native owner retains the same Arc<File>, event, stack-stable OVERLAPPED and fixed output until terminal completion. It polls with GetOverlappedResult(FALSE). Deadline or disconnect requests cancellation. Late completion cannot update any port or handle. A dedicated-owner unwind guard also retires the request before those stack resources drop. Thread-creation failure releases the reservations before issuing native I/O. A stuck driver can occupy one owner and permit indefinitely. Capacity exhaustion degrades optional stats and permits serial opens. No shared queue mutex or native waits on the device worker were introduced for the probe.

Existing worker operations, including ordinary data/control cancellation and Pending::drop with GetOverlappedResult(TRUE), can still stall that worker. A long-running printer operation can also delay wire delivery. The 100 ms deadline limits baseline adoption, not transport latency under those conditions. The code forwards native controller counter changes while logically open. Original-client equivalence (E04) and validation with a Windows driver remain open.

## Evidence

- An exact pre-change native-output completion fixture returns 43 where a logical-open origin of 40 requires 3.
- Linux pure component passes 19 tests, including six-counter subtraction, wrap, malformed/unsupported origin, retained permits through late completion, cancellation once, cancel-success ordering, and fixed read-only control shapes.
- A controlled Linux dispatcher runs the verbatim production Opening/Serial section. It verifies guessed IDs and duplicate CREATE rejection, two-port successful CREATE reply order, availability fallback and fresh reopen origins. The fixture substitutes endpoint and native probe implementations.
- Actual Windows native component passes MSVC-target cargo check with tests. Linux whole helper passes 76 unit tests and 10 worker tests. No Windows executable ran and no COM device or original vendor application was used.
- Validation ran in private version 0.1.972 with helper termcraft-rdp-win-stats972. The initial Linux build referenced the previous fixture binary name in a compile-time integration test. Correcting that private test reference enabled the full helper run. An initial MSVC check reused the prior private build cache. Final checks used this candidate’s own MSVC cache. No app build, main source change or toolchain installation.


# FIX1: terminal wait before exclusive output slice

Status: DONE_WITH_CONCERNS (implementation and checks complete; Windows device execution remains open).

The reviewed candidate passed the output Vec through coercion to `&mut [u8]` before its cancellation callback called GetOverlappedResult(TRUE). The kernel could still be writing that allocation. `cancel_and_collect` now takes a zero-argument terminal wait and a separate output converter. It requests cancellation, waits to terminal completion without constructing a slice, and only on success passes an exclusive slice to the converter. `Pending::cancel_and_retire` captures its original per-open statistics origin before waiting and applies it in that success converter. Native failure bypasses conversion and preserves the native status. Cancellation still happens once; a successful operation winning cancellation still produces its actual bytes/count. Unbounded ordinary data/control cancellation and Pending::drop waiting are unchanged and remain separately open.

Exactly three integration files changed relative to the reviewed candidate: `engines/rdp/src/serial_completion.rs`, `engines/rdp/src/serial_windows.rs`, and `tests/windows-serial-control-component/src/lib.rs`. The cumulative integration candidate is still eight files. MAIN was not edited; all eight baseline hashes were verified unchanged. No commits were created because this task produces a private candidate. The original candidate and its 35-entry seal remain intact and pass verification.

## Covering checks and exact results

Exact executable commands are in COMMANDS.md; complete output is preserved in the named logs.

- RED ownership fixture: `python3 .../order_fixture.py .../source` failed with `AssertionError: terminal wait must not receive an exclusive native-output slice` against the original reviewed code (`red-order.log`). This source proof covers the ownership ordering that output-only tests cannot detect.
- RED Rust API fixture: the focused cargo command with `+1.95.0`, offline locked dependencies and two jobs failed with E0593 (expected one argument, zero supplied) and E0061 (five parameters versus six) against the original reviewed API (`red-rust.log`). This is API incompatibility evidence, not a successful runtime reproduction of undefined behavior.
- GREEN ownership fixture: exit 0, `PASS: zero-argument native wait precedes success-only slice transformation with captured origin` (`green-order.log`). It verifies the production helper and Windows call site, success-only transformation, captured origin and blocking wait boundary.
- GREEN focused component: `CARGO_BUILD_JOBS=2 ... cargo +1.95.0 test ... --offline --locked -j 2` exited 0; **22 passed, 0 failed**, plus 0 doc tests (`green-rust.log`). New covering cases: `terminal_native_write_precedes_origin_subtraction`, `native_failure_preserves_output_and_skips_stats_conversion`, and `cancellation_then_terminal_wait_then_conversion_happen_once_in_order`. The terminal-write fixture simulates native completion using the pointer retained at submission and checks all six resulting statistics words. Updated existing tests cover old-origin/reopen, abort/other error, read/write success, purge and event masks.
- Windows native production extraction: `CARGO_BUILD_JOBS=2 ... cargo check ... --offline --locked -j 2 --target x86_64-pc-windows-msvc --tests` exited 0 with installed rustc **1.99.0**, first attempt, in 8.40s (`windows.log`). Two existing warnings: deprecated fetch_update in serial_stats.rs, unused metadata in extracted drives.rs. No compiler crash occurred in this scoped check.
- Full Linux helper: exactly one `CARGO_BUILD_JOBS=2 ... cargo +1.95.0 test ... --offline --locked -j 2` invocation exited 0; **76 unit tests and 10 worker integration tests passed** (`linux-helper.log`). Build completed in 1m41s, tests in 1.14s and 4.13s respectively. Existing dead-code warnings appear in the full log. No repeated broad build loop was attempted.
- rustfmt check exited 0 (`format.log`). Both delta and cumulative patch dry runs exited 0 (`fix1-dry-run.log`, `cumulative-dry-run.log`). Applying each patch in temporary directories reconstructed all eight exact payload hashes (`reconstruction.log`). Prior seal verification exited 0 (`prior-seal-check.log`, 35 OK entries).

## Private identity and integration packet

Private source VERSION/Cargo app/helper version: **0.1.9877**; app name **termcraft-win-stats-fix1-9877**; helper name **termcraft-rdp-win-stats-fix1-9877**. The focused fixture crate retains its inherited component package identity. Source-only identity and the worker integration-test binary reference are private build inputs, not integration changes.

`integration-packet.md` names the delta patch, cumulative patch, payload, reviewed baseline, cumulative base, full hash maps and source inputs. `hash-map.json` proves the relationship between MAIN baseline, cumulative base, reviewed candidate, FIX1 source and FIX1 payload. `source-inputs.sha256.json` captures every private source input. `manifest.json` records identity and helper binary SHA-256. `evidence.sha256` seals the packet and logs.

## Limits and self-review

Self-review found no further scoped changes needed. Windows binaries were typechecked but not executed, no real COM device was used, and Miri was not run. The original vendor-client equivalence check remains open. Full app build and unrelated dispatcher fixture re-execution were not performed for this three-file fix; the inherited evidence remains clearly marked in the preceding report. Existing unbounded cancellation remains separately open, as instructed. Parent owns scoped reviewer dispatch; no subagent/reviewer was spawned.
