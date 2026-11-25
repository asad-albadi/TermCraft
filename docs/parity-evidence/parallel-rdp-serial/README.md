# D02 embedded serial provider

The embedded RDP helper now sends an MS-RDPEFS port capability and announces each explicitly mapped COM port through the existing device redirection channel. Drives and serial ports can coexist on the same channel. The server identifies a serial mapping by device ID with an empty create path. The local worker opens each selected TTY before the first certificate attempt and holds that descriptor across certificate retries and for the life of the session. Remote opens duplicate the bound descriptor, so replacing a path or recycling its device identity cannot redirect the session to a new local endpoint. A disconnected device now requires a new session to bind a replacement.

Unix endpoints support byte reads and writes, baud-rate queries and changes, and line-control queries and changes. The worker returns an explicit status for unsupported commands. Read-only mappings permit reads and supported queries using a read-only descriptor and preserve the current local line settings. Writable handles use raw mode and restore their previous terminal settings on close. Nonblocking requests share a bounded queue and use a one-second I/O deadline. Expired requests complete before touching the port. Close can cancel pending reads even when requests fill the ordinary queue, and a terminal hangup returns device disconnected.

## Validation

- The locked, offline helper build passes in [helper-build.log](helper-build.log).
- All 27 helper tests pass in [certificate-retry-green.log](certificate-retry-green.log), including ten serial regressions and the existing drive and clipboard tests. Serial tests send serialized requests through the production static-channel processor on Linux and exchange bytes with an owned pseudo-terminal. They cover real baud changes and restoration, line-control queries, read-only data and mutation denial, close cancellation at queue capacity, the documented metadata probe, drive and serial ID routing, refusal of a non-TTY character device, recycled PTY isolation, expired I/O leaving bytes untouched, terminal hangup, and a certificate retry after the configured path is retargeted.
- [red.log](red.log) captures the initial missing API and [create-probe-red.log](create-probe-red.log) captures the documented metadata-only create request failing before its access mode was supported. [review-red.log](review-red.log) captures data reaching a recycled PTY, an expired write sending bytes, and a hung-up terminal producing a successful empty read before those fixes. [certificate-retry-red.log](certificate-retry-red.log) captures the replacement PTY receiving bytes before bindings were shared across connection attempts.

Physical serial hardware, native Windows RDP server end-to-end interoperability, and macOS operation remain unverified. The embedded Windows helper has no serial provider. Modem, flow-control, purge, and configurable communication-timeout IOCTLs remain unsupported. Embedded printer and smartcard providers, writable-drive rename and delete, and the outstanding host and original-product comparisons remain open.

## Protocol sources

- [MS-RDPESP specification and examples](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/04ae8f6b-a2fe-4989-9314-09bff11fa086)
- [Serial port announcement example](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/fabd29bc-57e2-4f3b-95c8-671bdebd25b1)
- [MS-RDPEFS port capability](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/ecfacbbb-329b-4f5a-bad1-ad7123bb6910)
- [MS-RDPEFS create reply](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/99e5fca5-b37a-41e4-bc69-8d7da7860f76)
- [Linux devpts identity and index reuse](https://github.com/torvalds/linux/blob/master/fs/devpts/inode.c#L441-L493)
