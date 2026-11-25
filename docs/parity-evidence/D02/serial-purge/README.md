# D02 Unix serial PURGE increment — 2026-11-25

This source increment implements IOCTL_SERIAL_PURGE (0x001B004C) for the existing explicitly bound Unix serial provider. It is not D02 completion, native Windows/macOS runtime validation, or physical serial validation. No physical devices, saved configuration or credentials were accessed or changed.

## Behavior

The four documented bits are accepted in any nonempty combination: TXABORT=1, RXABORT=2, TXCLEAR=4, RXCLEAR=8. Unknown bits and an empty mask return STATUS_INVALID_PARAMETER. Input must be exactly four bytes; output is empty. Existing writable-remote-handle authority and stale-handle checks run before side effects.

RXCLEAR/TXCLEAR map to tcflush(TCIFLUSH/TCOFLUSH); combining both uses TCIOFLUSH on the retained opened terminal descriptor. Native errors propagate through the existing tty status mapper. Only after a successful clear, or for abort-only requests, the provider marks already admitted requests matching this device, remote handle and selected direction for cancellation. The next ready pass returns one cancellation completion for each marked request, including requests blocked behind another read/write. The purge completion remains separate and succeeds before cancellation completions are drained through the existing worker loop. No asynchronous Unix native I/O buffer is in flight at this point.

Clearing the terminal queue does not erase data already accumulated in an owned protocol read buffer or abort requests without the abort bits. Abort-only requests do not flush unread terminal data. Later admitted operations remain usable. Completed operations retain their original result and byte count. Other opened ports, handles and directions remain unaffected. Existing timeout snapshots, direction ordering, pinned endpoint identity and restoration logic remain unchanged.

## Evidence

- `red.log`: seven new production-dispatch/owned-PTY tests failed at the intended unsupported-PURGE assertions against the original implementation. Zero passed; 47 existing unit tests were filtered. The original source plus these tests is preserved privately as `red-serial.rs`.
- `red-mapping.log`: the new pure mapping test failed against a deliberately unsupported selector stub: abort-only flag 1 expected no terminal flush, received unsupported. The intermediate source is preserved privately as `red-mapping-serial.rs`.
- `green-purge.log`: all eight focused tests passed. They cover every valid mask's selector and invalid masks; actual unread PTY input removal with fresh-byte transfer afterward; matching-direction abort of multiple admitted requests; new requests after purge; stale handles and an actually read-only binding; preservation of partial protocol read bytes and queued writes under clear-only masks; successful completed-write bytes; abort-only retention of unread terminal input; and another separately opened PTY's queued read/data.
- `green-helper-full.log`: **55 unit tests and eight private PC/SC fixture integration tests passed, zero failed**, with no filters. The original 47-unit baseline remains passing. The two existing options dead-code warnings remain; no new compiler warning was emitted.
- `setup-missing-loader.log` is an initial harness-copy error (missing actual loader_env.rs dependency), not behavioral RED. Copying that unchanged production dependency repaired the private harness.
- `commands.txt` records exact commands. Every Cargo invocation was offline, locked and limited to two build jobs. Scoped rustfmt check passed.

The private full helper has package version 0.1.916 and the unique normal binary `termcraft-rdp-events916`. Its manifest/lock and PC/SC integration-test binary environment lookup were adjusted only in the private harness; those identity changes are excluded from the integration patch. The candidate input manifest records the copied production dependencies. The integration patch includes only serial.rs and this new evidence directory. No MAIN source was edited.

## Limits

The tests observe actual Linux PTY RX clearing, queue ownership and cancellation. They do not establish physical UART FIFO flushing, hardware TXCLEAR behavior under flow control, modem state or native macOS runtime behavior. TCIFLUSH/TCOFLUSH selectors are checked as pure mappings; PTY data already transmitted to its peer cannot be retracted. A failed terminal clear does not mark protocol requests for abort. Abort-only requests operate on the provider's pending queue and do not probe whether physical hardware is connected.

GET/SET_WAIT_MASK, WAIT_ON_MASK, queue sizing, communication status/properties/statistics and other previously unsupported controls remain explicit STATUS_NOT_SUPPORTED. Poll readability alone is not used to fabricate receive-event history or modem events. Windows source is unchanged; no Windows component compile was required or claimed for this Unix-only increment. The root coordinator owns fresh review, integration and full application gates.

## Primary references

- [MS-RDPESP Server Device Control Request](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/824cc217-febd-4a2e-a95f-5b5bdc32848f): serial PURGE wire control number.
- [IOCTL_SERIAL_PURGE](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_purge): mask size, independent cancellation/clear families, invalid-mask status and separate cancelled-request completion. This provider follows the permissive Serial.sys/SerCx flag combinations, not SerCx2's additional clear-with-abort restrictions.
- [PurgeComm](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-purgecomm): authoritative numeric flag assignments and distinction between buffer clear and pending operation termination.
- [GNU C Library POSIX line-control functions](https://snapshots.sourceware.org/glibc/trunk/2026-02-17_16-27_1771345621/manual/html_node/Line-Control.html): tcflush selectors discard only unread input/untransmitted output on the opened terminal. The Open Group tcflush page returned HTTP 403 during source consultation; this primary implementation manual supplied the POSIX operation contract.
