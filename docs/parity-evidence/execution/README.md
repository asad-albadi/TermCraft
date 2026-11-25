# Dependency-based execution and desktop/serial boundary

The full approved 60-point outcome is unfinished. `PARITY_EXECUTION_QUEUE.json`
is the single queue. `scripts/parity_queue.py` validates coverage, states,
explicit blockers and dependency cycles; it lists implementation tasks whose
interfaces are available. A partial result or rebuild status `Ready` cannot
close that backlog. The command returns a decision; it is not a scheduler.

Craft Anything 0.6.1 runner tools/settings were inspected: start/resume/status,
rebuild checks, refinement rounds and score thresholds are exposed. No
backlog-aware continuation hook or runner source exists in this workspace.
Automatic Craft Anything continuation has not been configured. The native
Codex goal records the user's requested objective as active; this does not
establish background scheduling capabilities.

## D02: Linux allocation

`d02-allocation-red.log`, `d02-allocation-close-red.log` and
`d02-allocation-query-red.log` reproduce unsupported allocation, retained
unused reservations, and incorrect allocated-space reporting respectively.
`d02-engine-green.log` exercises the actual device request dispatcher over
real files: allocation without extending EOF, signed-length/access checks,
last-handle/disconnect cleanup and native allocated-byte queries. Handles
share reservation ownership by file identity, including handles opened later.
Explicit close reports cleanup failure; abnormal worker shutdown performs
best-effort cleanup. Linux filesystems lacking fallocate/punch support report
errors; Windows/macOS allocation is still unavailable. Identity-safe
rename/delete and printer/serial/smartcard channels are still missing.

Reference: [MS-FSCC FileAllocationInformation](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/0201c69b-50db-412d-bab3-dd97aeede13b).

## D03: classic embedded RFB transport

`src/desktop/vnc.rs` uses the D01 common session/frame/input contracts and an
owned cancellable Tokio worker. Existing saved VNC definitions keep the
external adapter by default. Embedded mode is reachable through session
settings, requires explicit classic-unencrypted consent, and never selects
another security type as a fallback. Classic password challenge buffers and
DES key schedule use zeroization; secrets never enter saved session JSON or
process arguments. Passwords longer than eight ASCII bytes fail explicitly.
The RDP vault is not reused for an unverified VNC identity.

`vnc-tests.log` drives the actual handshake and protocol loop with fragmented
bidirectional byte streams (Tokio duplex, not socket-based interoperability):
known pixels, pointer and CopyRect bytes, 800x600 DesktopSize, oversized
frames, mid-rectangle disconnect, read-only input rejection, incoming
Latin-1 clipboard direction, explicit security selection and password wait
outside the network timeout. The challenge response is checked against an
independent OpenSSL DES vector. Canvas/fullscreen/input are shared with D01;
`settings-ui.log` uses real egui pointer events for the new controls.

Open D03 work: TLS/VeNCrypt, Unicode clipboard extensions, additional
encodings/cursor coverage, actual TCP server/desktop UI and native/original
interoperability. Classic text is Latin-1; unsupported Unicode upload fails
before queuing rather than disconnecting. Server-driven resize is supported;
client-driven resize is explicitly unavailable, and the canvas scales locally.
No D03 implementation checkpoint or full parity is claimed.

Reference: [RFC 6143](https://www.rfc-editor.org/rfc/rfc6143.html).

## P04: serial settings and lifecycle

New saved line settings preserve legacy `baud` and default to 8N1/no flow
control. The session editor exposes data bits/parity/stop bits/flow control.
Native POSIX termios and Windows DCB mappings reset inherited handshake flags;
all configuration calls are checked. Reader shutdown observes cancellation
under idle and full-queue conditions, does not send empty heartbeat buffers,
and reports device-removal/read failures. The queue remains bounded.

`serial-red.log` records the missing settings/model/API before integration.
`serial-green.log` exercises defaults/roundtrips, invalid values before open,
actual termios mappings and a test-owned PTY transport. Physical parity/flow
and native driver evidence are separate, still pending.
`windows-serial-source-check.log` checks the real settings module against the
Windows target; the small check crate is source composition only. The full
app Windows check failed at a dependency because `lib.exe` is absent
(`windows-check.log`), so no full Windows build is claimed.

Reference: [Microsoft DCB](https://learn.microsoft.com/en-us/windows/desktop/api/Winbase/ns-winbase-dcb).

## Host boundary and prior evidence

The latest persisted D02 provider host logs all pass and remain untouched.
They predate this source. The fresh boundary `build_command_run` invocation
of `cargo test --offline --locked --no-fail-fast` was rejected with
“MCP tool call requires approval, but approval policy is never.” No retry,
policy investigation or bypass was attempted. Run this directory's
`run-host-validation.sh` through the existing authorized host route to retain
new source hashes plus unfiltered helper/root logs separately.

`library-tests.log` records one fresh restricted-route failure in the existing
`unauthorized_x11_never_connects_to_local_server` listener test (44 passing at
that time); it is not a current full-suite pass or a new policy investigation.
No tests were ignored or weakened. Native/original comparison and full
checklist completion cannot be inferred from a build or rebuild screenshot.


## Continued D03: verified VeNCrypt TLS

After the first successful rebuild boundary, development continued with
VeNCrypt 0.2 X509None/X509Vnc. The embedded default is now verified TLS;
classic None/password modes remain explicitly unencrypted and consented.
TLS verifies the configured endpoint hostname using system trust plus optional
bounded PEM roots. Anonymous TLS, unsupported subtypes and security fallback
are refused. Resumption is disabled so each connection verifies its peer.
The platform verifier performs validation; no accept-all verifier is used.

`vnc-tls-red.log` records the absent mode API; `vnc-tls-green.log` passes ten
actual protocol tests, including real rustls encryption over duplex streams.
The TLS fixture offers insecure None alongside VeNCrypt and observes selection
of 19; valid X509None and X509Vnc yield exact green pixels, while untrusted,
wrong-name and rotated self-signed certificates prevent authentication. These
are real TLS/parser operations, not TCP/native interoperability evidence.
`settings-ui-tls.log` passes six UI tests and `build-tls.log` records the locked
root build. Unicode clipboard, broader cursor/encoding and real TCP/native
server/UI evidence remain open. No completion count changes.

Source: [VeNCrypt 0.2 extension specification](https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst#vencrypt).
