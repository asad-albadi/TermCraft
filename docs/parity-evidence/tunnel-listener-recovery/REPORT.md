# Tunnel listener fixture recovery (private 937)

Ready for root review; nothing applied to MAIN. Payload changes exactly `tests/tunnel_interop.rs` and `tests/support/ssh_server.rs`. `payload.patch`, `base-sha256.json`, and `payload-sha256.json` are the scoped handoff. `main-ownership-check.json` confirms both MAIN files still match their captured bases. Patch dry-run succeeded on both paths.

## Cause and proof

The old `port()` bound port zero, read the assigned port, and immediately dropped the listener. An owned SSH fixture could select that released port before the forward bound it. SshServer's existing startup mutex coordinated SSH starts with other SSH starts only. `connect()` accepted any successful TCP connection before the SSH subprocess had necessarily reported ExitOnForwardFailure.

`reproduction.log` was run against the original fixture plus a deterministic diagnostic test: a private owned banner listener binds the exact selected forward address before the real `ssh -R` starts. The existing `connect()` returns this wrong endpoint, reads exactly `SSH-2.0-O`, and the actual SSH subprocess subsequently reports `remote port forwarding failed for listen port 37697`. The echo receives zero payloads. This reproduces the reported bytes without guessing how frequently bind(0) recycles ports. It demonstrates a fixture race, not a product Tunnel.args defect.

The retained final negative test controls scheduling further: the PTY subprocess first blocks on an explicit input gate, existing readiness connects to the occupied requested address, and the test then releases the gate to exec real SSH with the same ordinary fixture arguments. Real SSH must reject the occupied port and echo count must remain zero. There is no timing sleep or SSH retry in this schedule. The adversary intentionally bypasses the cooperating fixture allocator to prove genuine port conflicts remain visible.

## Repair

A PortReservation holds the selected kernel listener until handoff and retains a numeric logical claim until drop. The shared process-local registry coordinates SSH server ports, tunnel forward ports, echo ports, conflict blockers, and intentionally unavailable destination ports. Numeric claims conservatively span address families, including IPv6 dual-stack collisions. Allocation rejects a port already claimed even when the OS can currently bind it during handoff, cancellation, or rebind. The allocator mutex covers only registry insert/remove; no test is serialized and no lock is held over SSH negotiation or payload traffic. The prior SSH-startup mutex is unchanged.

SshServer retains its claim through restarts and cleanup. Tunnel configurations retain their claim through subprocess cancellation and the original rebind checks. Echo retains its claim until its worker joins. All original exact-payload, SOCKS, IP family, explicit loopback bind, Linux wildcard restriction, payload count, failure, cancellation and rebind assertions remain. Production process code, tunnel argument construction, Mosh, and runtime inputs are unchanged.

## Evidence and results

- Actual RED: `red-claim.log`, exit 101: one test fails because a second fixture claims the exact released forward port. The RED implementation removes only the ownership rejection from the otherwise same PortReservation; retained in `red-ssh_server.rs` and frozen `red-test`.
- Final GREEN: `final-v2-all.log`, exit 0: 14 passed, zero failed/ignored, test threads 14. This includes the original 11 tests and three new tests: deterministic wrong-endpoint/SSH-conflict negative, exact-candidate claim lifetime/release regression, and controlled six-worker IPv4/IPv6 Local/Remote/Dynamic handoff integration.
- The controlled parallel test holds all six selected forwards open logically before allowing any to bind, verifies all 18 SSH/echo/forward claims are distinct while live, then releases all workers. Every real SSH roundtrip carries exactly TUNNEL_OK once, then cancels and rebinds. The controller/worker channels have bounded waits so failed startup cannot leave an unbounded barrier.
- Shared helper compatibility: `shared-support.log`, exit 0: ssh_authentication 4 passed (plus its one-test subprocess), ssh_interop 3 passed/1 pre-existing qualified Dropbear ignore, ssh_lifecycle 15 passed. Lifecycle cases include real host-key replacement/restart. The initial attempt lacked the SSH matrix include resource in the private snapshot; that snapshot-only compile issue was corrected before these results.
- Normal private binary built successfully, `normal-build.log`, and `--help` exited 0, `normal-help.log`. `normal-binary.json` identifies it.
- Frozen final test SHA256: `98afba964bcb9591ee34a1493491fa4f8aeea6f54093981273cb6a36c1e5c6c7` (`final-test-v2`). All stages' frozen hashes are in `frozen-binaries.json`. Running executables were copied before invocation and never overwritten during their runs.
- Owned SSH server logs are retained without keys/config: `reproduction-sshd/`, `green-sshd/`, `final-sshd/`, `final-v2-sshd/` (22 final fixtures), and `shared-support-sshd/` (24 fixtures). SSH trust remains fixture-owned and isolated with `-F /dev/null`, pinned private known_hosts and GlobalKnownHostsFile=/dev/null. All jobs and owned fixtures were reaped; no fixture sshd remains.

## Scope and source identity

Snapshot uses canonical `scripts/package-release.py::source_inputs(root)`, complete tests and README/FEATURE_PARITY, plus the compile-time ssh_interop matrix resource: 702 hashed base files. `baseline-sha256.json` is the complete map. `private-changes.json` lists exactly the two payload paths plus private app/RDP/Wayland manifest+lock identities. App package version 0.1.937 and binary termcraft-tunnel-fixture937; independent engine package/binary identities also use 937. None of these private identity edits are in the payload.

Claims coordinate cooperating fixtures within one test executable. They are not host port ownership or cross-process networking isolation; unrelated external services can still produce a genuine bind conflict, which tests continue to expose. No host service, namespace requirement, network policy, global sshd config or user credentials were changed. No parent/full gate or native Windows/macOS check was run. Root should freshly review and gate after integration.
