# P03 embedded legacy terminals

- `red.log` records the initial Cargo integration test failing because the embedded protocol API was absent.
- `green.log` records `CARGO_BUILD_JOBS=2 cargo test --offline --test legacy_protocols -- --nocapture` on the ordinary Linux account. The account cannot bind TCP ports 512–1023, and the fixtures assert that the embedded client reports this failure.
- `reserved-ports.log` records the same Cargo-built fixture binary with loopback enabled inside an isolated Linux user/network namespace. Both BSD handshakes reached the configured destination port with the configured remote username and a TCP source port in 512–1023. The namespace supplied the bind capability locally without changing the host settings.

The TCP fixtures cover Telnet negotiation, fragmented IAC and escaped payloads, terminal type, NEW-ENVIRON USER, negotiated window sizes, carrier reuse, refusal of direct fallback for SSH-gateway profiles, Rlogin urgent window-size negotiation, Rsh command transmission, cancellation during handshake, and close with full input and output queues.

The embedded Rsh client selects the protocol's single-stream mode with combined stdout/stderr. Rlogin uses the terminal's usual raw input behavior, so local software flow-control mode switching and traditional client-side tilde commands are outside this implementation. The reserved-port checks reflect actual socket binding; a particular server can still reject the handshake using its own account and host trust policy.

The fixtures do not substitute for interoperability tests against external daemons or native Windows/macOS validation.

Specifications used: [Telnet](https://www.rfc-editor.org/rfc/rfc854), [NAWS](https://www.rfc-editor.org/rfc/rfc1073), [terminal type](https://www.rfc-editor.org/rfc/rfc1091), [NEW-ENVIRON](https://www.rfc-editor.org/rfc/rfc1572), [BSD Rlogin](https://www.rfc-editor.org/rfc/rfc1282), and [FreeBSD rshd](https://man.freebsd.org/cgi/man.cgi?query=rshd&sektion=8).

## Integrating Enter, echo, and Rlogin flush

`review-red.log` records the additional fixture compilation failing before the Enter and output-flush APIs existed. `review-green.log` and `review-reserved-ports.log` record 11 passing cases on the ordinary account and in the isolated namespace, respectively.

The Telnet fixture verifies that typing and Enter send a complete CRLF-terminated line, writing a literal CR still sends CR NUL, and local echo stops and resumes with peer ECHO negotiation. The privileged Rlogin fixture queues obsolete data both in the client and in TCP, sends an urgent flush marker, and verifies that the transport discards old data and exposes the following output in order. The UI uses the flush notification to discard any partially consumed queued chunk.

## Preserving Enter after a same-frame paste

`deferred-enter-red.log` records the GUI fixture observing an extra LF on the wire after Telnet switched to BINARY while a paste was queued. `deferred-enter-green.log` records the focused regression passing, and `deferred-enter-paste-regressions.log` records all 10 matching desktop paste tests passing.

The loopback peer verifies the exact order of paste text, ordinary typed bytes, Enter, and subsequent typed bytes in the same frame. It checks CR in BINARY, CRLF in NVT, and a single logical CR in macro recording. The existing desktop tests exercise paste confirmation, cancellation, and line delay.
