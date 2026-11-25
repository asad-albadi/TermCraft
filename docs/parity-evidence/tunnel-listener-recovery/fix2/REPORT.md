# Tunnel fixture claim lifetime

The prior full gate failed at tests/tunnel_interop.rs:452, the first same-address kernel bind after release. The process-local claim protects cooperating fixtures only, not independent binders or automatic client-side kernel allocation. The specific outside occupant from the full run was not identified.

The deterministic red harness extracts the original test and actual fixture claim helper, then adds a synchronized independent binder immediately after socket release. Its real loopback bind succeeds and the original same-address bind fails with OS error 98, matching the full gate. The harness is evidence only, not part of the patch.

The revised test uses PortReservation::bind for claim-aware allocation, then takes the owned listener so its fixture reservation has no socket. The underlying socket stays continuously bound while a duplicate is submitted to the real registry and rejected as AddrInUse. Dropping the first reservation makes the same port logically claimable through the original handle. No fresh exact-port kernel binds or guesswork are involved. The extracted focused test passes. Two temporary harness mutations independently prove the test fails if taking the socket clears the logical claim too early or if the reservation destructor leaks the logical claim.

The payload is one hunk in tests/tunnel_interop.rs. It leaves tests/support/ssh_server.rs, production, parallel gating, all exact payload checks, wildcard checks, cancellation, and real tunnel rebind checks untouched. It does not change the real SSH/sshd handoff or claim to reserve a handoff against unrelated processes. Cargo identity edits are private build scaffolding, excluded from the payload.

Final verification: red and both mutations exited 101 as expected. Extracted focused green exited 0 with 1 passed. The final Cargo run exited 0 with 14 passed, 0 failed, 0 ignored at 14 threads. Its logs show all 18 distinct claimed fixture ports and exact payload/count/cancellation/rebind evidence for all six parallel workers. The scoped patch dry-run passed. No MAIN files were written by this task.
