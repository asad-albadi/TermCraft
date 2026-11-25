# D02 bounded Windows serial native owner

Approved design: one process-limited (16), shared-binding-exclusive owner per logical open. Reserve before spawn; spawn before any logical-open native calls. Owner initializes, probes optional origin, and issues all data/control calls. Shared worker uses bounded nonblocking requests (18 queue slots, 16 ordinary active operations plus wait-mask escape and CLOSE). CLOSE requests cancellation once, forwards actual terminal outcomes, restores state after pending operations and stats probe retire, then completes. Session drop disconnects channels without joining. Stuck native calls retain owner, file, buffers and binding capacity indefinitely.

Ruling: retain binding exclusivity through optional read-only baseline probe retirement before state restoration, even after the 100ms origin-adoption deadline. This makes close/reopen wait on a stuck probe while other devices remain responsive.
Ruling: baseline bind-time CreateFileW/GetCommState remains setup work outside the shared worker; no claim is made that initial mapping construction is nonblocking. Logical-open GetCommState/GetCommTimeouts moves to owner.

Implementation plan: first establish exact-source controlled Win32 fixture and observe old worker blocking on stalled close; then replace worker-owned Pending lifecycle with bounded owner/session lifecycle, verify exact-source cancellation/late terminal/drop/capacity/mask escape/initialization cases, and real MSVC --tests typecheck; finally seal cumulative and delta patches/hash maps. No MAIN changes or app build.

## Implementation and status

DONE_WITH_CONCERNS: bounded actor implementation and local covering checks complete; Windows hardware/runtime validation remains unexecuted. No reviewer/subagent was spawned. Parent owns independent review.

Only one product source file changes relative to accepted Stats FIX1: `engines/rdp/src/serial_windows.rs`. Five new test-component files ship with it. The complete payload contains those six delta paths plus seven unchanged accepted statistics paths. `owner-delta.patch` is the integration patch for the parent's accepted Stats FIX1 baseline. `cumulative-original-baseline.patch` is archival and must not be applied after Stats FIX1 integration.

`Session` retains only bounded request/result endpoints and logical identity on the shared worker. One owner thread creates its Handle, queries initial configuration/timeouts, runs optional origin initialization, issues every data/control request, polls completions, requests cancellation once and restores native state. No data/control I/O is submitted before owner thread creation succeeds. Per-binding exclusivity is shared by all Binding clones and survives certificate retries. Sixteen process-wide owner permits are reserved before thread creation. An owner and its binding remain occupied indefinitely when the driver fails to retire I/O or blocks initialization/restoration. The existing statistics owner has its separate sixteen-probe cap; total owner-thread capacity is bounded by the sum of these two caps.

The request queue holds at most eighteen requests; native pending admission allows sixteen ordinary requests and one additional SET_WAIT_MASK escape. CLOSE has its separate protocol admission allowance and is asynchronous. A full or disconnected owner request queue produces a busy completion carrying the original request ID; it never waits for room on the shared worker. The result queue is also bounded (nineteen), and only the dedicated owner can wait for its consumer. Session/Serial destruction disconnects endpoints without a native wait or thread join.

CLOSE preserves actual terminal outcomes, including a successful write that beats cancellation and an actual aborted read. It completes after pending responses and restoration. Kernel-owned output is interpreted only after native terminal success, using the Pending request's captured logical-open statistics origin. On disconnect the owner cancels once, retains all native buffers/events/file state, and waits for terminal retirement before restoring. Native Pending's blocking Drop remains solely an unwind fallback on the issuing owner thread; it is no longer reachable from the shared worker or Serial's destruction. Handle's optional-probe wait and restoration are similarly confined to that owner.

Successful CREATE replies still publish through the explicit FIFO Opening queue. IDs are unpublished until their successful CREATE reply, duplicate opens are rejected, and shared worker/printer polling code is unchanged. State initialization now occurs asynchronously before CREATE success. The owner stays alive through native retirement so its issuing-thread exit cannot cancel operations early.

Ruling: logical IDs are reserved before asynchronous initialization, so failed initialization can leave gaps. IDs remain monotonic and are never reused; the cost is only non-contiguous internal IDs.

## TDD and covering evidence

Exact commands and complete output are in COMMANDS.md and the log files.

RED: the exact original Serial/Pending source, with only controlled native/OS boundaries, compiled successfully. `fixture/check --exact stuck_close_does_not_block_shared_dispatch_and_terminal_write_wins --nocapture` failed (exit 101) with `CLOSE blocked shared dispatcher while native cancellation was pending` in 0.10s (`red.log`). The fixture released the stalled operation and joined its test worker before asserting, so the red run left no hanging test worker.

GREEN: the shipped `windows-serial-owner-component` full crate test command exited 0 with **27 passed, 0 failed**, 0 doc tests, in 0.58s (`owner-component.log`). Eleven owner-boundary cases execute exact generated production Serial/Handle/Pending/probe bodies. The other sixteen tests are unchanged production control, statistics, serial-name and completion tests. Coverage includes:

- Native cancellation remains pending while the same controlled shared-dispatch loop processes another serial port and a real filesystem read. The eventual write success returns its actual byte count before CLOSE success.
- Serial drop returns promptly, cancellation is requested once, buffer/event destruction and restoration remain deferred, cloned bindings reject reopen, and late native abort permits restoration/reopen.
- Terminal read abort is forwarded with its native cancellation status before CLOSE, without fabricating successful output.
- Sixteen stuck owners occupy process capacity through disconnection; a seventeenth open fails busy before native I/O. Late completion releases the quarantined owners.
- Saturation with sixteen operations retains the SET_WAIT_MASK escape, single-wait behavior and CLOSE progress.
- GetCommState can be deliberately blocked during logical initialization while the shared worker remains responsive and guessed IDs remain unpublished; Serial drop returns promptly.
- Controlled owner thread-spawn failure releases both permit/binding and returns the resource failure before logical initialization or native I/O. A subsequent open succeeds.
- A late optional baseline probe cannot restore/reopen the binding until its native owner retires, despite CREATE's 100ms optional-origin timeout.
- A blocked native submission fills the eighteen-slot request queue. The next request gets an immediate busy result; every accepted/rejected completion ID is accounted for, and successful reads return the native-written bytes.
- CREATE responses retain admission order and duplicate/unpublished handle rejection.
- Actual actor GET_STATS processing subtracts its captured origin across all six words.

The controlled native boundary records the issuing thread for each operation and verifies that cancellation, terminal polling and event destruction remain on that same owner. It flags event destruction or same-port restoration while a native operation is still pending. The Win32/OS-handle adapter and owner thread-spawn boundary are substitutions; the test's dispatch loop is scaffolding, not a claim of full RDP transport execution.

A development fixture initially reported early restoration in the sixteen-port case because its check incorrectly treated another port's live I/O as belonging to the restoring port. The boundary was corrected to correlate native file handles; the product code was unchanged. Complete final assertions cover each operation's port and issuing thread. This was fixture instrumentation, not an ignored product failure.

The inherited pure completion/control/statistics component full test command exited 0 with **22 passed, 0 failed**, 0 doc tests (`pure.log`). The Windows native production component passed `cargo check --target x86_64-pc-windows-msvc --tests` on its first invocation with installed rustc 1.99.0 in 7.31s (`windows.log`). Three warnings remain: deprecated fetch_update in the existing stats permit and new owner permit, and unused metadata in the extracted drive component. `msvc-input-match.log` confirms the final product source is exactly the source checked by that command. rustfmt check exited 0 (`format.log`).

No broad helper/app build was run for this Windows-only continuation while the root full gate was active. No Windows executable was run, no real COM driver/device or vendor client was used, and Miri was not run. Those checks remain unexecuted rather than inferred from the Linux boundary fixture or typecheck.

## Integration and seals

Private VERSION/app/helper version: **0.1.9878**. Private normal app binary name: **termcraft-win-owner-9878**; helper: **termcraft-rdp-win-owner-9878**. No app/helper binary was built for this continuation.

`baseline-map.json` pins every accepted Stats FIX1 input (new test files have null base hashes). `hash-map.json` maps accepted base, final source/payload, original cumulative base and observed MAIN hashes, and explicitly marks unchanged statistics paths as outside the owner delta. Both delta and archival cumulative patches reconstruct all thirteen exact payload hashes (`reconstruction.log`). `source-inputs.sha256.json` records all private source inputs including generated fixture inputs. The accepted Stats FIX1's original 672-entry seal still passes (`prior-seal-check.log`); its report/manifest/seal were retained as prior-* copies. A separate `evidence.sha256` seals this candidate.

This worker made no MAIN edits. The parent is concurrently integrating accepted Stats FIX1 and a separate Linux serial change, so there is deliberately no claim that MAIN remained identical to the original pre-Stats baseline. Root must preflight/apply the owner delta against the accepted Stats FIX1 hashes. Parent-controlled unrelated changes are outside this candidate.

## Self-review and limitations

Self-review checked cancellation-once behavior, source-buffer terminal ordering, restore/permit release before CLOSE acknowledgement, thread-spawn failure, bounded channels, CREATE publication order, per-binding clone quarantine, wait-mask escape and preservation of per-open origins. Product code no longer places a native Pending or Handle on the shared worker. No further scoped issue was found.

A driver may retain one bounded owner indefinitely, keeping that port unavailable. Initial bind-time native mapping construction remains outside the shared worker and can still block setup. Dedicated-owner restoration can also stall and retains the owner/binding slot. These are explicit limitations, not UI/shared-worker waits. Idle published sessions currently keep the worker's existing short completion polling active. Independent review and real Windows execution remain outstanding.
