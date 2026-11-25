# Unix serial PURGE independent review — ACCEPTED

Reviewed 2026-11-25. No actionable correctness, specification, authority, ordering or test-quality finding.

## Exact identity and ownership

All base-manifest paths match the private base and MAIN at review start and end. All candidate-manifest and integration-owned-manifest paths match the frozen worker candidate. Candidate-input differs from candidate only in engines/rdp/src/serial.rs, as intended; its original serial hash matches the base. The helper-only manifest/lock/PCSC binary lookup identities differ from MAIN and are excluded from integration. The integration patch paths exactly equal integration-owned-manifest; replaying it against the private base reproduced every owned file hash. The sole production delta is serial.rs, base 87b0c5ca792b2703d3d190ffe8d7ecdba25290ba32dad80978487d35fd254efb to candidate 2730a2be6382e93f2d0256a2390750e8605b0c92213a0064e92b8257e2d5f4de. See identity-verification.json and patch-check/.

Fresh private helper source preserves the candidate serial hash exactly. Only private Cargo.toml/Cargo.lock package identity and pcsc_worker binary lookup were changed to version 0.1.920 and normal binary termcraft-rdp-purge-review920. Fresh normal binary SHA256: 85a92d07351a4e7d11e0d23c238117dda5998e91794c3c1d7b8ffe63392fe398. Worker 916 binary was not reused. No MAIN production edits were made.

## Independent validation

Fresh normal helper build passed. All eight focused PURGE tests passed, zero failures (47 original unit tests filtered). Full private helper then passed 55 unit tests and eight private PC/SC integration tests, zero failures/filters. Scoped rustfmt passed. Only the two pre-existing options dead-code warnings appeared. Exact commands and logs are adjacent; all Cargo invocations were offline, locked, jobs=2, with explicit warm target directory. Builds/tests ran sequentially, with no same-identity rebuild during testing. All command sessions completed and were reaped. No full application gate was repeated.

## Specification and code review

The consulted primary references confirm wire IOCTL 0x001B004C, TXABORT=1, RXABORT=2, TXCLEAR=4, RXCLEAR=8; nonempty flag combinations; ULONG input and no output payload; cancellation completion separate from purge completion; Serial.sys/SerCx permissive clear/abort combinations. All 15 masks map correctly to no tcflush, TCOFLUSH, TCIFLUSH or TCIOFLUSH. Zero/unknown flags and malformed sizes reject before effects.

The matching live remote handle check precedes control dispatch; writable authority precedes native flush and protocol cancellation. control returns the existing tty_status error on failed tcflush; the pending-mark loop is reachable only for an Ok control result, so flush failure cannot mark aborts. This error ordering is established by source review, not a fault-injection runtime claim. Cancellation is restricted to already admitted jobs matching device, remote handle and requested direction. The cancelled flag is checked before per-direction waiting, allowing all selected queued jobs to complete exactly once; removal from the deque prevents duplicates. Later jobs start with cancelled=false. Completed responses and byte counts cannot be changed, because completed jobs are already removed. Old handles and other ports are filtered explicitly; existing stale-handle disposal remains unchanged.

The production worker sends the immediate purge response before calling serial.ready and sending cancellation responses. Clear-only requests leave owned partial-read bytes and protocol-queued writes intact, and abort-only requests do not call tcflush. Retained descriptors, timeout snapshots and existing restore-on-drop behavior remain unchanged. Added PTY loops have one-second deadlines; the shared PTY master is nonblocking, so read_exact cannot indefinitely block. Test-owned file descriptors use RAII, and independent tests/full helper completed promptly.

## Limits

Linux owned-PTY behavior and code/selector semantics only. No physical UART FIFO/TX flow-control, Windows/macOS runtime, native Windows change or D02 completion claim. The full application parent gate remains root-owned.

## Primary references checked

- https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/824cc217-febd-4a2e-a95f-5b5bdc32848f
- https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_purge
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-purgecomm
- https://snapshots.sourceware.org/glibc/trunk/2026-02-17_16-27_1771345621/manual/html_node/Line-Control.html
