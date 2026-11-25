# Independent tunnel listener fixture review

Verdict: **ACCEPTED**. No actionable findings. No MAIN source changes; no broad/root gate executed.

Reviewed packet: `../tunnel-listener-fixture/payload.patch`, exactly `tests/tunnel_interop.rs` and `tests/support/ssh_server.rs`. Captured base hashes equal current MAIN and packet base files. Applying the patch to those bases independently produces the exact two payload hashes. The supplied `final-test-v2` SHA256 independently matches `98afba964bcb9591ee34a1493491fa4f8aeea6f54093981273cb6a36c1e5c6c7`.

## Findings

The repair addresses fixture ownership at selection/handoff rather than weakening product assertions. A physical listener covers selection and registry insertion; the process-local numeric claim survives socket release. Numeric exclusivity across IPv4/IPv6 is conservative and appropriate for possible dual-stack listeners. Reservation errors drop their candidate sockets without removing another owner's claim. Drop first closes any retained socket, then removes its own claim. Poison recovery does not introduce nested registry locks. Registry locking covers insert/remove only; the existing SSH startup mutex is unchanged, and no registry lock survives into SSH startup, negotiation, payload transfer or cancellation. There is no new lock-order cycle.

SshServer retains its reservation across host-key restart and kills/waits before releasing it. Echo retains its reservation until the worker has joined. Forward configuration ownership spans subprocess teardown and rebind assertions, including repeated cancellation/relaunch. Deliberately unavailable destinations remain claimed through all modes. Conflict blockers retain real listeners. The conflict test's deliberately replaced tunnel port is covered by the blocker reservation while the configuration's original unused claim is harmlessly retained.

All original exact payload, count, SOCKS, address-family, explicit loopback, Linux wildcard restriction, failure and cancellation/rebind checks are retained. The negative regression uses a pre-exec input gate and an owned listener to prove the wrong endpoint bytes precede actual SSH bind failure, without scheduling assumptions. The six-worker schedule pauses after all handoffs, checks 18 live unique claims, then runs all IPv4/IPv6 Local/Remote/Dynamic real SSH payload/count/cancel/rebind checks. No production Tunnel or Process changes, retries, credential changes or assertion relaxation appear in the patch. Pinned fixture-owned authentication options remain unchanged.

Coordination is explicitly limited to cooperating fixtures inside one executable. Unrelated processes/services can still cause genuine bind conflicts. This is consistent with the claimed scope, and the deterministic adversary verifies such failures remain visible.

## Independent evidence

- `owner-final-v2.log`: copied/hash-verified owner's frozen binary, 14 passed / 0 failed / 0 ignored, 14 threads. Includes the wrong endpoint `SSH-2.0-O`, real SSH forwarding failure, 18 unique claims and six real roundtrips.
- `build.log`: independent private package 0.1.941, unique normal binary `termcraft-tunnel-review941`, separate RDP/Wayland identities, offline/locked/jobs 2. Build succeeded in 57.22s; warnings only.
- `tunnel_interop.log`: independently rebuilt/frozen binary, 14 passed / 0 failed / 0 ignored, 14 threads; same deterministic schedule and all original checks.
- `ssh_authentication.log`: 4 passed plus the existing one-test child subprocess passed.
- `ssh_lifecycle.log`: 15 passed, including actual server and gateway host-key restart/recovery cases.
- `normal-help.log`: separately frozen normal app executable built by Cargo's integration-test build; `--help` exit 0.
- `cleanup-check.json`: 67 retained owned fixture logs across both tunnel runs and helper runs; zero surviving owned fixture processes or temporary fixture roots. All execution/build children completed and were reaped.

Snapshot used canonical `scripts/package-release.py::source_inputs(root)` plus complete tests, README.md, FEATURE_PARITY.md and the required SSH matrix include resource: 716 current MAIN inputs. Snapshot differences are exactly the two payload files and six private manifest/lock identity files (`private-changes.json`). lib/main mtimes were explicitly refreshed after copy2. The normal and test executables were copied before runs; no frozen executable was overwritten while executing. `snapshot-sha256.json`, `validation.json`, `frozen-sha256.json`, `artifact-sha256.json` and raw logs identify reviewed inputs/results. MAIN scoped hashes remained unchanged at finalization.

The owner's additional ssh_interop 3-pass/one-existing-ignore result was inspected as packet evidence; it was not independently repeated. No native Windows/macOS validation or broad integration gate is claimed. Root should integrate the exact reviewed patch and run the planned gate.
