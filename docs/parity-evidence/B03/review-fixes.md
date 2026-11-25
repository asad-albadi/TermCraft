# B03 scoped ownership-fix review

Reviewed only the changed private `src/services/sshd.rs` and the appended ownership
fix evidence. Candidate SHA-256:
`581de972806e24b4bc5f4a3bc885da2f1bb1317a0d8df1fa2c9e8d9a7912a6ba`.
The recorded original SHA-256 in `review-fix-base-hashes.json` is
`ffa402fa2ff2dc6fa94deccdf689fde007e0fd900597cfef8434ebee921705ec`.
UI, packaging and shared integration patch retain their prior review scope.

**Disposition: original Important finding resolved; no new Important/Critical
finding established in the scoped SSH delta.**

The actual russh-sftp processing-loop handler now owns `SftpLease`, with admission
released only on handler destruction after its open/staged handles are dropped
(`src/services/sshd.rs:576–581`). Admission counts the per-channel controls until
that completion marker is set (`:310–312`), including initialized SFTP sessions.
Channel close/EOF and parent-handler destruction signal those controls. The stream
polled by the actual detached library loop registers cancellation and its timer
before delegating I/O (`:500–510`); this wakes idle reads and terminates the real
loop, rather than merely terminating its scheduling future. The one-hour bound
now belongs to that stream (`:552–568`).

The owner-provided RED log reproduces the previous retained-four/fifth-admission
failure. GREEN evidence contains three passing live ownership regressions covering
four retained sessions, six close/reopen cycles, parent disconnect with unfinished
upload/client handles retained, and actual library-loop cancellation/deadline
cleanup. The supplied post-fix adapter/supervisor log records 10 + 17 passing tests;
the formatting log is empty with the owner recording success. Inspection found no
unresolved uncertainty requiring duplicate test execution. This reviewer ran no
Cargo commands or live services and made no source edits.

Root still owns narrow rebasing onto newer B05/P06 shared files and the merged
verification gate. Prior Linux host-ABI qualification and unavailable native
Windows/macOS evidence limits remain unchanged.
