# Linux allocation cleanup repair — 2026-11-27

The inherited close/disconnect test failed on hosted Ubuntu. An owned ext4 loop fixture reproduced the production defect: reserving 1 MiB with KEEP_SIZE on an eight-byte file allocated 2048 blocks; punching a hole starting at EOF left all 2048 blocks. A same-length truncate reclaimed the unused tail, leaving two blocks. Original and grown/unaligned-EOF regressions failed before the repair.

Cleanup now takes a short Linux write lease before reading EOF and truncating to that length. Independent readers/writers, unsupported filesystems or unavailable safe notification setup cause cleanup to retain allocation. A pending lease break also prevents reclamation. Notification uses per-file SIGURG and restores prior per-file settings; no global signal handlers or host sysctls change. Remote handles close before the shared reservation attempts cleanup.

This is conventional lease exclusion, not atomic EOF truncation. Linux can forcibly revoke a lease after its configured timeout (45 seconds on the test host); prolonged suspension across the short check/truncate section can exceed that guarantee. Do not claim unconditional reclamation with concurrent local access.

## Evidence on fix branch

- Full engine: 86 unit plus 10 integration tests passed.
- Private ext4: 86 unit tests passed using the existing Python-capable owned Arch fixture image. The initial minimal image lacked Python and failed two printer tests; this environment error was corrected, not suppressed.
- Btrfs: 86 unit tests passed five default-parallel repetitions; one representative full log is retained.
- `lease-break-red.log`: removing only the pending-break guard makes the production cleanup test fail. External-opener regression also failed before lease protection.
- `fork-lease-probe.c`: independent CLOEXEC descriptors inherited between fork and exec temporarily prevent lease acquisition. Exclusive cleanup tests now create their files inside isolated single-test children; the ordinary suite still runs in parallel.
- Ext4 fixture uses disposable owned Docker containers and a private loop image with cleanup traps. No user filesystems were mounted or repaired.

These runs used Rust 1.96.1 and CARGO_BUILD_JOBS=2 on fix commits 3ee2330 and 12369e9. The source differs from the final combined Windows/macOS candidate; its fresh checks are recorded in the joint integration ledger and hosted CI. The exact Ubuntu runner kernel/filesystem is verified by hosted CI, not inferred from these local runs.
