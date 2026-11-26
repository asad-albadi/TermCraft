# Apple Silicon native XDMCP

The TCP owner lives in TermCraft. The existing private arm64 XQuartz runtime
still receives only its inherited Unix listener. This path uses the same
compiled inventory, signed binary, Unix setup-cookie checks and process-group
lease as ordinary managed X11. No system XQuartz, preferences, display
manager, startup script, or system network configuration is changed.

The worker snapshots a finite pinned image once per session, reserves a single
IPv4 TCP address before sending UDP and allows at most eight concurrent
native sessions and 24 X clients per listener. An 8-second negotiation/admission
deadline follows the private XQuartz launch. UDP frames have bounded length
and retries, TCP setup has a 2-second deadline, and the manager IP and
MIT-MAGIC-COOKIE-1 are checked before any X traffic reaches the Unix socket.
TermCraft substitutes a separate private backend cookie and waits for real X
setup success before publishing readiness. Cookies are not logged. This mode
does not negotiate display-manager authentication and both protocols are
unencrypted, so it uses explicit per-session network consent.

In Direct mode the initial authorized X connection anchors the session.
Its loss closes additional X clients and stops the owned display. Indirect
allows a forwarded manager, and one chooser disconnect at the initial
manager triggers another INDIRECT_QUERY on the same UDP socket. The TCP
reservation stays in place, but the old X clients, authority and backend
are retired. The successor starts a new process from the same verified
image and accepts only its own new authorization. Tab cancellation does
not block the UI. Normal application exit drains cancelled workers for
up to three seconds. Forced host-process termination is outside this
shutdown path.

## Owned Debian xdm replay

Run on the Mac in a GUI session. Set the first two IPv4 values to the owned
guest and the Mac address the guest can reach. Leave the display unset for
automatic allocation, or set `TERMCRAFT_TEST_XDMCP_DISPLAY=180` to reserve
TCP 6180. The harness releases the session automatically after its requested
hold (5–180 seconds).

```sh
TERMCRAFT_TEST_XDMCP_ENDPOINT=192.168.64.2:177 \
TERMCRAFT_TEST_XDMCP_LOCAL_IPV4=192.168.64.1 \
TERMCRAFT_TEST_XDMCP_MODE=direct \
TERMCRAFT_TEST_XDMCP_HOLD_SECONDS=60 \
TERMCRAFT_TEST_XQUARTZ_ROOT=/path/to/TermCraft.app/Contents/Resources \
cargo +1.96.1 test --locked --test macos_xdmcp \
  macos_xdmcp_external_owned_manager -- --ignored --nocapture
```

1. Direct should print `EXTERNAL READY` after the real xdm opens its
   X connection. Check the Debian Xsetup marker for the advertised display,
   and verify the login window or the authenticated mapped fixture. Keep the
   manager alive until the test prints `EXTERNAL CLEANUP`.
2. For Indirect, configure the owned primary's Xaccess with a chooser rule and
   an owned target that can receive FORWARD_QUERY (standard UDP 177 on a
   second manager address for a forwarded peer). Run the same command with
   `TERMCRAFT_TEST_XDMCP_MODE=indirect` and a longer hold if needed.
   Select the target in the chooser. Require both `EXTERNAL HANDOFF` with a
   new XQuartz PID and the selected manager's Xsetup marker. Check a visible
   authorized window and `EXTERNAL CLEANUP`. A WILLING reply, or an Indirect
   connection that never leaves the primary, is not evidence of chooser
   handoff.
3. The driver does not start or stop xdm. Capture diagnostics that identify
   opcodes and results without recording cookie bytes.

## Qualification on Apple Silicon macOS 26.6.2 (25G83)

- Owned loopback peers exercised Direct source correlation, both X11 byte
  orders, cookie-less and wrong-cookie rejection, a mapped green reference
  pixel, and child/authority/listener cleanup when the initial X client closed.
  Indirect exercised the second query on the same UDP return address, a new
  manager endpoint and cookie, rejection of the retired cookie, a fresh
  XQuartz PID, a mapped green pixel, and explicit owned stop.
- The isolated listener and hostname-cancellation cases exercised opt-in,
  occupied-port refusal before opening package bytes, no ready state for a
  silent manager, and shutdown cleanup while leaving the peer's UDP socket
  alone. The four cases were replayed with 256 descriptors from a renamed
  bundle layout using the extracted package's Resources (4 passed in
  26.91 seconds). A replay overlapping the independent cross-VM run had a
  private Unix readiness timeout before XDMCP negotiation. The serial replay
  passed. Simultaneous host GUI launches are not qualified by this evidence.
- The Rust library suite reported 266 passed, zero failed, two ignored and
  15 filtered by the browser/Chromium/Finder exclusion. The macOS build and
  source contract tests passed (five cases), as did two platform layout
  checks. The unchanged private Unix XQuartz owner replay and actual SSH
  X11 forwarding test each passed at 256 descriptors against the new
  extracted package. The Linux Xephyr archive test and one packaging layout
  test were skipped because their opt-in inputs/platform were not available.
- The package replay used the locally built arm64 app and the previously
  built RDP helper. The unmodified packager verified the exact XQuartz
  inventory and signatures, and its startup manifest checker verified all
  1,800 extracted payload hashes. The test ZIP SHA-256 was
  `3fce491988cc0b2bb4769cab14bb4b080753f4470a4a70936fc48537e45d46e1`.
  This verifies the pinned nested XQuartz signature and package contents,
  without qualifying an outer app distribution signature or notarization.
  The XQuartz executable remains
  `30cff793431ef36178d9814eeb13024331c2acc28c7daf608ce575fd3a9893cd`
  and the compiler inventory remains
  `0fd1bb3ffd162962691d4b7c0b6283b53e448b6a50a51c502c86074b5317f171`.
- The owner of the Debian ARM Tart fixture reported that real xdm 1.1.11
  Direct authenticated and mapped xlogin at `192.168.64.1:174` from manager
  `192.168.64.2:177`. The guest's Xsetup marker and the Mac cleanup assertions
  passed. With UDP 1177 the independent primary also authenticated and mapped
  its Indirect chooser. The pinned XQuartz does not advertise XTEST, so the
  chooser automation used by the Linux Xephyr test cannot select a target on
  this engine as-is.

Protocol reference [X.Org XDMCP specification](https://xorg.freedesktop.org/releases/X11R7.6/doc/libXdmcp/xdmcp.html).
