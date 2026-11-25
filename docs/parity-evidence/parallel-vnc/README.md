# D03 embedded VNC evidence

The fixture binds an ephemeral IPv4 loopback listener, runs the actual RFB 3.8 exchange over TCP, and drives the production `DesktopSession` API. It verifies the exact RGBA output, Hextile colour reuse across tile boundaries, key and pointer packets and release, server-driven resize, Rich Cursor hotspots and transparency, directional clipboard restrictions, and cancellation during an incomplete update. The supplied-carrier case uses an unresolvable logical hostname and demonstrates that the carrier is used without looking up that hostname.

Additional cases verify Unicode clipboard in both directions using the Extended Clipboard capabilities and notify/request/provide messages. Incoming data uses an independently generated Python zlib vector with Japanese, a CRLF line ending and emoji. The fixture independently decompresses outgoing Arabic/emoji data and checks the UTF-8 length and NUL terminator. Bound checks reject oversize frames, invalid rectangles, unsupported encodings and over-expanding clipboard data. The classic Latin-1 compatibility case checks a visible Unicode error followed by successful clipboard use on the same session.

The existing [RFB transport tests](../../../src/desktop/vnc_tests.rs) cover the three security modes involved in the scenarios (classic None, classic VNC password, and verified VeNCrypt with both X509 subtypes), the certificate and hostname failure cases, and no replacement security selection after rejection.

- [Initial TCP regression run](red.log) shows the original missing Hextile and Extended Clipboard negotiation. Existing TCP resize and view-only cases passed at this boundary.
- [TCP session run](green.log) uses `CARGO_BUILD_JOBS=2 cargo test --offline --test vnc_session -- --nocapture`.
- [Transport and TLS run](unit-green.log) uses `CARGO_BUILD_JOBS=2 cargo test --offline --lib desktop::vnc -- --nocapture`.

Protocol behavior follows the [RFB specification](https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst) for Hextile, Cursor and Extended Clipboard. The client advertises Rich Cursor for interactive sessions. Clipboard capability negotiation advertises plain UTF-8, with a zero unsolicited size and only actions allowed by the profile. Incoming compressed text is bounded before and during decoding. Classic peers continue using Latin-1.

These transport results cover the in-repository loopback fixture and TLS test streams. Native Windows and macOS endpoints, third-party VNC server interoperability and comparison with the original product remain separate validation work.

## Review fixes

- [VNC regression before the fix](review-clipboard-red.log) timed out waiting for Notify when a receive-only peer advertised Text, Notify, and Provide, with a zero unsolicited size. The corrected negotiation lets that peer request and receive Unicode data over TCP.
- [Serial regression before the fix](review-serial-red.log) detected queued writes flushing after close on a Linux PTY. The corrected close cancels pending writes, releases the serial probe immediately, and reports completion after both workers have finished. The regression keeps the session object alive and observes the PTY report that all slave handles are closed.
- [Focused verification](review-green.log) runs the serial, shared process input, and VNC integration suites with `CARGO_BUILD_JOBS=2 cargo test --offline --test serial_settings --test process_input --test vnc_session -- --nocapture`.
- Windows serial ports now request exclusive access as required by the communications API. The [Windows source check](review-windows-source.log) compiles the actual serial settings, serial lifecycle, and process input modules in a minimal crate for `x86_64-pc-windows-msvc`. The physical COM-port behavior remains to be run. [Microsoft communications resource handles](https://learn.microsoft.com/en-us/windows/win32/devio/communications-resource-handles)
