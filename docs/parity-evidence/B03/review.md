# B03 independent review

Reviewed the private candidate `target/agent-work/b03`, its `owned-files.json`,
`shared.patch`, `base-hashes.json`, implementation evidence, approved services
platform B03 plan and parity contracts. This is a source/evidence review; no
production files were edited and no application-producing Cargo command, live
service, or broad test suite was run by this reviewer.

## Important — SFTP workers escape channel accounting and lifetime ownership

**Location:** private `src/services/sshd.rs:450–455`, with channel admission at
`src/services/sshd.rs:304–307` and task retirement at `:441`.

The task placed in `self.tasks` awaits `russh_sftp::server::run` inside a one-hour
timeout. In the pinned russh-sftp 4.0.0, `run` (`src/server/mod.rs:99–104`) calls
`run_with_config`, which spawns the actual request-processing loop and returns
immediately (`:108–126`). Consequently the tracked outer task finishes while its
SFTP session remains active. The next subsystem request removes that finished
handle, and channel admission no longer counts the still-open SFTP channel.

An authenticated client can repeatedly open a session channel, request SFTP, wait
for initialization, and retain the resulting stream. It can exceed the advertised
four active channels and multiply the per-session 32-handle/4096-directory-entry
budgets without bound. The one-hour SFTP timeout and session Drop's task aborts
also target the already-finished wrapper, not the actual worker. Destruction of
the whole service runtime still provides eventual service-stop cleanup; that does
not restore connection/channel accounting or individual SFTP task ownership.

Retain a permit/cancellation handle for the actual request-loop lifetime (for
example through owned stream/handler guards), or use a serving API whose future
actually owns that loop. Keep active SFTP channels in admission accounting until
their processing ends, and close/abort their real work on channel/session teardown.
Add a focused regression that retains four initialized SFTP sessions and verifies
that a fifth session channel is rejected; include cancellation/cleanup of an
unfinished upload while its parent connection is closed.

## Reviewed behavior and evidence limits

No other Important/Critical issue was established in this review. Source inspection
covered explicit password/signed-key allow/deny checks, host-key creation/reuse and
SFTP-root overlap exclusion, capability-root/no-follow path operations and atomic
no-replace upload publication/rename, shell opt-in and pending channel accounting,
owned Iperf child cleanup, sealed verified packaged executable bytes, explicit
installed-provider selection, UI background startup and transient password fields,
and source/license/digest/package inventory staging. Root must apply only narrow
shared hunks to preserve its newer P06 packaging hooks.

Owner evidence records 10 adapter tests, 17 existing supervisor tests, three Python
packaging tests, and formatting checks as passing. These do not exercise the
retained simultaneous-SFTP-channel case above. The payload remains qualified only
for Linux x86_64 on the documented glibc 2.44/GCC 16 host ABI, included through
`--iperf-runtime`; unavailable Windows/macOS native execution is not passing
evidence. Full merged verification remains root's gate.

**Disposition:** changes required for the Important finding before acceptance.
