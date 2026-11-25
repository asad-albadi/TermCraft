# Cron/NFS integration recovery — 66.7% (40/60)

Scope: the three failures in `parallel-integration-cron-nfs/full.log`. No checkpoint or whole-gate success is claimed. No full integration gate was launched by this recovery. Main source changes are limited to four files in `recovery.patch`; `recovery-sha256.json` records the exact review cut and immutable GUI test binary.

## Baseline integrity

All four saved pre-edit files match the failed gate's `source-sha256.json`; `recovery-base-proof.json` records this. An inventory comparison after the first three recovery edits found only those three files changed, establishing no other preserved, unreported source changes. Cargo identity remains main `0.1.0`; no private package metadata or lockfile was copied. Private app-producing build window was granted to X03 and released after it copied its own app; these recovery builds use hashed `--lib`/`--bin` unit executables only.

The Cron failure is at **initial** `start_cron`, line62, after the test's save. An initial informal report incorrectly described it as post-stop; checking numbered source corrected that interpretation. Current source and the gate inventory are consistent. This is not evidence of stale compiled source.

## 1. Cron ownership survives a copied file description

Rust's file lock is held on the open file description on this host. A concurrent fork/pre-exec child can inherit the descriptor before CLOEXEC closes it. Dropping the parent's File then does not release its lock until the child's last copy closes. Thus `save_jobs` can return while the following initial scheduler start still encounters the old lock. The merged code already awaited Stopped for restart and released its file before publishing Stopped; adding more lifecycle sleeps would miss this cause.

Added a deterministic regression holding a duplicated description across owner teardown (dup and fork share the relevant kernel description). Before the fix, it fails exactly at reacquiring the retired owner's lock: `recovery-lock-red.log`, exit101. The test also checks live owner exclusion and that closing the old duplicate cannot unlock a replacement owner.

Fix: a private StoreLock guard explicitly unlocks in Drop before closing the file. Active scheduler/save exclusion is unchanged; all existing early-return paths receive the same release behavior. No retry or timeout was added.

Final Cron test run: **10 passed, 0 failed**, exit0; `recovery-cron-green.log`. This includes real initial start/restart/disable/corrupt preservation, descendant cancellation/deadline, owner exclusion, durability barrier, concurrency cap and new copied-description regression.

## 2. SCP GUI startup competes with owned process fixtures

Isolated SCP passed in2.02s. Original unchanged parallel GUI bin suite also passed140/0 with6 ignored (`recovery-gui-parallel-baseline.log`), so that alone did not explain the recorded failure.

Pinned actual SCP, four Chromium GUI fixtures and the owned SSH agent fixture to one allowed CPU with16 test threads. The unchanged SCP fixture again hit its existing8s deadline: **5 passed, 1 failed**, exit101, `recovery-scp-contention-red.log`. This reproduction ran while another one-CPU GUI exploration was active, increasing scheduling contention. All browsers and SSH were real owned fixtures. The exploratory full GUI one-CPU run accidentally used libtest's default single worker; it was stopped after verifying it had no child processes and is explicitly not a passing result (`recovery-gui-1cpu-baseline.log`).

Fix: acquire the already established owned-process fixture slot in the SCP test before server creation and before its stage timer. That slot previously covered the four Chromium fixtures and owned SSH-agent test, but omitted this SSH/Python-helper fixture. Only these process-heavy fixtures share it; the rest of the suite stays parallel. The8s assertion and production timeout behavior are unchanged.

Final constrained GUI result: **10 passed, 0 failed**, exit0, in27.77s (`recovery-scp-contention-green.log`). The final run includes all six original contention fixtures plus four managed-server GUI tests, with16 test threads on one CPU. It uses the copied immutable test executable identified in the SHA manifest.

## 3. Telnet socket completion after stop

Production `serve_tcp` already closes its listener and drains client workers before publishing Stopped. However a concurrent pre-exec child can hold an inherited listener descriptor briefly after the service worker drops its own copy. This is the same OS inheritance interval already documented by the application's existing PTY hangup regression.

A deterministic fork diagnostic holds the copied listening description: after parent close, TCP connect succeeds; after child close/exit, connect is refused. The child was reaped. Results: `recovery-listener-inheritance.json`. This provides a separate reproduction of the observed completion interval; it does not claim a deterministic reproduction of the entire GUI scheduling race.

Fix: after observing Stopped, the GUI test polls actual TCP refusal within the **same four-second deadline begun before the Stop click**, then retains its refusal assertion. This measures OS completion and keeps the lifecycle budget rather than sleeping or extending it. Real pointer Stop, state, restart, live listener, sibling preservation, UDP and password non-persistence assertions remain.

Final managed-server GUI run: **4 passed, 0 failed**, exit0, `recovery-listeners-green.log`.

## Validation boundary

`cargo fmt --all -- --check` exited0 (`recovery-format.log`). Builds use CARGO_BUILD_JOBS=2, offline locked dependencies. No production root lock, cancellation or authentication behavior was weakened. Cron's explicit ownership release is the only production change; both GUI fixes are test-only.

Final constrained GUI run completed0: **10 passed, 0 failed**, in27.77s. Every command started by this recovery has completed; the mistaken serial exploration was stopped and explicitly excluded, with no owned child processes at termination. No full latest-source pass, native Windows/macOS execution, or independent review is claimed.


## Review follow-up — shared Telnet deadline

Independent review (`recovery-review.md`) accepted Cron and SCP, and found one P2 in the first Telnet test fix: wait_state still owned a separate deadline, late Stopped/refusal could bypass timing assertions, and blocking connect could exceed the remaining budget. That initial test fix and its older validation results are superseded by this follow-up. The approved Cron/SCP source hashes are unchanged.

Only `src/ui/managed_servers.rs` changed after review. `wait_state_until` now receives the same absolute deadline created before the Stop click, and checks it after polling/rendering before accepting even an immediately available expected state. `wait_refusal_until` checks the remaining time before every probe, gives connect_timeout at most min(remaining,50ms), and checks the shared deadline after the probe before accepting ConnectionRefused. Timeout/WouldBlock/Interrupted retry within the existing deadline; other errors fail rather than count as stopped. Neither a late immediate refusal nor a saturated listener backlog can silently satisfy the assertion. Both polling stages use that single original four-second stop budget; no listener production code changed.

A new helper regression deliberately delivers refusal after the deadline and verifies that a timeout must be retried before actual refusal. It failed against the extracted original refusal-loop semantics (`recovery-deadline-red.log`, exit101), then passed1/0 after the fix (`recovery-deadline-green.log`, exit0). Its three-millisecond delay intentionally exercises late completion rather than serving as a lifecycle readiness sleep.

Fresh final-source results: original Servers GUI4/0 (`recovery-listeners-review-green.log`), same ten real GUI fixtures with16 threads/one CPU10/0 in37.55s (`recovery-scp-contention-review-green.log`), helper regression1/0, formatting exit0. Final binary and source hashes are refreshed in `recovery-sha256.json`. `recovery.patch` remains based on the original pre-recovery source; `recovery-reviewed.patch` and `recovery-reviewed-sha256.json` preserve the first reviewed cut. All seven separately merged X03 files match their accepted handoff hashes (`recovery-postreview-unowned-preservation.json`); they are excluded from this patch.

All follow-up commands finished. No full gate or self-review performed; this correction awaits the parent's scoped independent re-review. Progress remains66.7% (40/60).
