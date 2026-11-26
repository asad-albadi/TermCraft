# macOS provider qualification ledger

The source-level provider contracts remain covered by focused tests for BGRA
frame padding and Retina coordinate conversion, bounded capture lifecycle,
late-callback ownership, keyboard/pointer release, RDP clipboard direction,
printer ownership, serial completion, smart-card capability negotiation, and
native file preallocation.

## System CUPS on Apple Silicon

The provider at `8391d41` failed `INIT` with `0xc0000001` on Sonoma
14.8.7. The same guest's CUPS 2.3.4 answered the provider-equivalent local
socket query with HTTP 200 and IPP `successful-ok`. The queue reported its
UUID, accepted jobs and advertised `application/vnd.cups-raw`. Darwin's
`/private/var/run` was `root:daemon` (gid 1) with mode `0775`, which the
provider had rejected before connecting. `3855f70` accepts that system
directory profile while retaining root ownership, symlink/type checks and
root `getpeereid` validation on the retained connection.

In the disposable guest, `lpadmin -m raw` was rejected by macOS. A unique
queue using `lpadmin -p QUEUE -E -v socket://127.0.0.1:PORT -m
drv:///sample.drv/generic.ppd` exposed the raw MIME type through a supported
queue. A loopback listener accepted only the owned job. The RDP helper ran
`--printer-worker PARENT_PID` with framed `INIT`, `READY`, `BEGIN`, two
`WRITE` frames, `FINISH`, and `ABORT`. Each returned zero, the worker exited
cleanly, and the listener received all 78 expected bytes (SHA-256
`83fb20a110d323d330d307cbb5e6648b0f8cacf3c0bcaa298c5472dec6417362`).
`lpadmin -x QUEUE` removed the fixture, and `lpstat -v QUEUE` confirmed its
absence. The locked native RDP helper suite passed 28 tests with zero
failures or ignored tests.

The same spool and cleanup checks passed with the helper extracted from
the exact `3855f70` qualification ZIP listed in the [arm64 ledger](arm64.md).

## RDP serial on Darwin PTYs

The native engine at `21d5471` ran the existing serial RDPDR packet
fixture on macOS 26.6.2 (25G83) with owned PTYs. `cargo +1.96.1 test
--locked --manifest-path engines/rdp/Cargo.toml` passed 54 tests, with
zero failed or ignored. The focused serial run passed 29 cases, including
12 RDPDR channel cases, 14 backend cases and three completion cases.
The separate PCSC integration target remains Linux-only.

Darwin accepted `tcgetattr` on the owned PTY after peer hangup. The
driver now checks the retained descriptor for hangup before client
open. For `/dev/null`, `tcgetattr` returned `ENODEV` while `isatty`
returned `ENOTTY`. Checking the TTY at admission produces the correct
unsupported status. Under XOFF, Darwin accepted bytes into the queue
while the peer still could not read them. The test verifies no delivery
before XON and checks the completion ID and byte count for both paths.

The fixture negotiated COM2 and verified bidirectional I/O, baud and
line settings, read-only operation, timeout, purge, RXCHAR wait/reset,
cancellation at capacity, termios restoration and the pinned TTY across
a certificate retry. Physical serial hardware and independent Windows
RDPDR interoperability remain open.

The native arm64 run also exercised live loopback RDP TLS/NLA, framebuffer
resize, clipboard direction and helper cleanup. The separate device component
passed 13 tests, including native file preallocation. The PCSC transport
component passed 11 owned software fixture tests, including Darwin orphan
reaping through launchd. The installed Chrome suite passed native pipe,
input, viewport resize, proxy authentication and TLS failure checks.

The following issue #2 acceptance evidence is still open:

- ScreenCaptureKit permission grant, denial, and revocation on macOS 14+;
- real capture and input control using the permission-controlled desktop;
- physical CUPS printer, PCSC smart-card, and serial-device workflows.

Public Chromium and release delivery are tracked in
[issue #3](https://github.com/asad-albadi/TermCraft/issues/3).

The focused source tests establish bounded ownership and error contracts only;
they do not qualify those external providers.


## Focused Apple Silicon workflow replay

On macOS 26.6.2, Rust 1.96.1 and SDK 26.2, the focused run at
`730349f3da77303ee5708331ffadee3407b52770` passed 150 tests, with zero
failures and one ignored packaged SSH/XQuartz opt-in case. The suite used
owned peers and disposable preferences. Among the timed workflows were:

- Vim, less and htop over actual PTYs. UI tests exercised native Command+V
  routing, asynchronous paste targeting, and detach, paint, rename and reattach
  with the same live PTY. The VNC peer exchanged Unicode clipboard text in both
  directions.
- Vault 6/6, SSH agent 8/8, SSH options 11/11, SSH lifecycle 14/14 and desktop
  gateway 10/10. Shared sources used owned HTTPS and SFTP endpoints, and session
  import and credential/agent UI paths ran against owned fixtures.
- The native app produced 1360×880 startup frames with disposable System, Light
  and Dark preferences. Mean grayscale luminance was 31.14, 243.85 and 31.13
  respectively. System matched the host’s Dark appearance.

The paste regression test includes a shortcut-only positive control before
supplying a native paste event. Its previous Ctrl+Shift+V input on macOS
could pass without reaching the Command+V handler. Physical Command-key input,
native window focus, live OS theme changes and external-display scaling
remain operator checks. Physical device and permission workflows remain above.

The native redirection and gateway suites were replayed with the exact
manifest-matching RDP helper extracted from the `3855f70` qualification ZIP.
`cargo +1.96.1 test --locked --test rdp_options --test desktop_gateway`
passed 12 redirection and 10 gateway tests. All four clipboard policies
negotiated with the owned peer and transferred Unicode in permitted directions.

The separate arm64 SSH/SFTP reconnect probe used the TermCraft library at
`6cb3b87` and Apple OpenSSH 10.3p1. The first client generation ended after
the owned HTTP CONNECT tunnel was cut. A new tunnel authenticated with the
same pinned host key and restored shell and SFTP access. A new download
returned all 90,001 bytes of the Unicode-named file. The server recorded
exactly two authentications. Both generations drained, the three listeners
closed and the fixture directory was removed. This covered a fresh explicit
connection after transport failure.
