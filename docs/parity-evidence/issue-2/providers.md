# macOS provider qualification ledger

The source-level provider contracts remain covered by focused tests for BGRA
frame padding and Retina coordinate conversion, bounded capture lifecycle,
late-callback ownership, keyboard/pointer release, RDP clipboard direction,
printer ownership, serial completion, smart-card capability negotiation, and
native file preallocation.

The native arm64 run also exercised live loopback RDP TLS/NLA, framebuffer
resize, clipboard direction and helper cleanup. The separate device component
passed 13 tests, including native file preallocation. The PCSC transport
component passed 11 owned software fixture tests, including Darwin orphan
reaping through launchd. The installed Chrome suite passed native pipe,
input, viewport resize, proxy authentication and TLS failure checks.

The following issue #2 acceptance evidence is still open:

- ScreenCaptureKit permission grant, denial, and revocation on macOS 14+;
- real capture and input control using the permission-controlled desktop;
- bundled Chromium and its resource/signature and sandbox qualification;
- CUPS printer, PCSC smart-card, and physical serial-device workflows;
- Verification of optional macOS native dependency delivery in release automation.

The focused source tests establish bounded ownership and error contracts only;
they do not qualify those external providers.
