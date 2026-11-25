# Independent E03 clipboard-provider review — 2026-11-25

Verdict: **accepted for integration as a partial E03 candidate**, subject to the root integrated verification gate. Critical findings: none. Important findings: none identified. E03 remains open; parity stays 40/60 (66.7%).

Reviewed the approved plan/contracts, candidate patch, VNC source-worker and per-client protocol integration, classic/extended clipboard codec, X11 ownership/conversion path, Windows CF_UNICODETEXT component, opt-in UI, fixtures and evidence. No production implementation was changed during review.

## Independent verification

- All nine base entries match MAIN, including absence of the three newly introduced files.
- All nine candidate file hashes match `target/agent-work/e03-vnc-providers` exactly.
- MAIN `patch --dry-run -p1` succeeds for `vnc-providers-private.patch`, without rejected or offset hunks.
- Reviewed owner logs show 20 library passes / 2 isolated ignored; one explicitly invoked private real X11 clipboard pass; one private native framebuffer/control regression pass; one UI pass / one ignored; one explicitly isolated Start/Stop UI pass; and successful Windows component checking. These are owner-run evidence, not executions repeated by this reviewer.

## Correctness and security assessment

Clipboard is independently opted in and defaults off. The source worker owns clipboard operations; network clients use the bounded input queue. View-only listeners export selected-source clipboard text but reject remote writes. The existing single-controller lease limits controlling clients; speculative stale-cache concerns across multiple simultaneous controlling clients do not apply to this implementation.

The X11 component uses only CLIPBOARD, a private InputOnly requestor/owner and typed UTF8_STRING/Latin-1 STRING properties. It bounds event work/property sizes, times out nonresponding owners, handles ownership change, and never interprets INCR/non-text properties as supported text. Remote writes replace ownership and clear publication state without echoing their value. Dropping the owned connection/window removes only its own selection ownership; foreign owners remain intact. The fixture exercises real selection conversion, default/classic and negotiated Unicode paths, STRING fallback, view-only rejection, no-echo and foreign-owner preservation inside the private compositor.

The RFB path bounds wire and normalized text, checks extension negotiation, requires requested Provide, validates compression and UTF-8/NUL/termination, and does not silently replace non-Latin-1 text for classic-only viewers. Retained clipboard buffers use zeroizing wrappers; data is not deliberately logged or persisted. Windows checks the selected input desktop, bounds GlobalSize, uses CF_UNICODETEXT and transfers GMEM_MOVEABLE ownership only on successful SetClipboardData. Native Windows blocking delayed-rendering behavior is explicitly unqualified, rather than represented as cancellable verified support.

The native and clipboard patches independently edit the same Cargo dependency feature line. Root integration must union native IpHelper/WinSock and clipboard DataExchange/Memory. No semantic conflict exists in their other owned production paths at this baseline.

## Explicit remaining qualification, not silently excluded

Windows runtime clipboard/capture/input/Stop behavior, native Wayland portal/PipeWire frames/control, macOS native capture/input/clipboard, browser/HTTP viewer behavior, monitor selection and exact original TightVNC parity remain open requirements. Host clipboard/input/framebuffer access and portal consent were not used in this review. Existing X11 transport deadlines bound individual transport operations; the documentation correctly does not claim a native Windows cancellation deadline. Supported text limits and unsupported INCR/COMPOUND_TEXT/non-text formats are disclosed.

No broad build/full-suite run, host access, permission dialog action or active command was left by this review. The root's fresh integrated gate remains required after merge/formatting.
