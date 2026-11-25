# Independent recovery review

Verdict: **changes requested** for one test deadline defect. The Cron production fix and SCP fixture change are acceptable within this review's scope.

Reviewed cut: `recovery.patch` SHA-256 `7c23d360b92a22eefadb5e99e0cb838e47988011713b06a0b928041f0ddc2a99`. The four current source files match `recovery-sha256.json` exactly. This review covers those four files only; separately arriving X03 changes are outside scope.

## Finding

**[P2] Enforce the shared Telnet stop deadline on all completion paths** — `src/ui/managed_servers.rs:576–590`.

The new deadline is created before the Stop click, but `wait_state` creates another four-second deadline after the click. Its expected-state branch returns before checking that deadline. The refusal loop checks the outer deadline only after a successful blocking `TcpStream::connect`; it skips the assertion entirely when the first probe fails. Consequently, if Stopped and refusal become observable after the outer deadline (for example after a scheduling pause), this test can pass despite exceeding its claimed shared four-second budget. Also, the blocking probe can itself outlast the deadline: a fork child holding a non-accepting listener can retain a full connection backlog while the loop continues opening connections.

Pass one deadline through the Stopped wait and socket-completion check, check elapsed time on successful completion as well, and bound each connection attempt by the remaining budget. If using `connect_timeout`, treat timeout as a retry/failure within that budget rather than evidence of refusal. This preserves the requested four-second lifecycle assertion without extending timing allowances.

## Accepted portions and evidence

- `StoreLock` retains exclusive ownership during save/scheduler work and explicitly unlocks only at owner teardown. Failed acquisition never constructs a guard, so it cannot unlock a live owner's lock. Save returns and scheduler early returns release through the same guard; normal scheduler teardown still drops the lock before publishing Stopped.
- The inherited `File` duplicate is intentionally not a `StoreLock`. Once the retired description is explicitly unlocked, replacement acquisition uses a separate file description; closing the old duplicate therefore cannot explicitly unlock replacement ownership. The new regression checks live exclusion, reacquisition while an old duplicate survives, replacement exclusion after that duplicate closes, and final release.
- SCP acquires the existing fixture mutex before fixture startup and the unchanged eight-second loading timer. Only the established process-heavy fixtures share this mutex; this does not serialize the whole test suite or alter production behavior.
- The Telnet change retains the pointer Stop action, Stopped observation and refusal assertion. The saved fork diagnostic supports the inherited-listener explanation, although it is not a deterministic reproduction of the entire original GUI failure. Allowing OS refusal to settle within the original budget is reasonable once the deadline defect above is corrected.

Reviewed the baseline proof, patch/manifest, source call sites and saved red/green logs. Recorded evidence shows Cron 10/0, managed-server GUI 4/0, constrained GUI 10/0 with 16 threads on one CPU, and formatting exit 0. SCP contention RED used additional concurrent exploration, as disclosed; the RED/GREEN runs are therefore not an identical-load comparison, but the focused fixture exclusion remains appropriate. No full gate, native Windows/macOS result, or new passing execution is claimed by this review. No builds, broad reruns, or production edits were performed.
