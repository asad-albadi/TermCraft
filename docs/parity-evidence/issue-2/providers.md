# macOS provider qualification ledger

The source-level provider contracts remain covered by focused tests for BGRA
frame padding and Retina coordinate conversion, bounded capture lifecycle,
late-callback ownership, keyboard/pointer release, RDP clipboard direction,
printer ownership, serial completion, smart-card capability negotiation, and
native file preallocation.

The following issue #2 acceptance evidence is still open and must not be
substituted with synthetic tests:

- ScreenCaptureKit permission grant, denial, and revocation on macOS 14+;
- real VNC/RDP sessions with display capture and input control;
- Chromium/WebKit helper startup and sandboxed clipboard behavior;
- CUPS printer, PCSC smart-card, and physical serial-device workflows;
- Intel and Apple Silicon release artifacts with complete native dependency
  closure.

The focused source tests establish bounded ownership and error contracts only;
they do not qualify those external providers.
