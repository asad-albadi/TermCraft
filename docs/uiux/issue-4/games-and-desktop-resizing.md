# Games and remote desktop pane sizing

Owner-requested follow-up to issue #4. PR #8 stays draft and unmerged; full human acceptance remains open.

- [x] Center all thirteen active game boards within the remaining game surface and fit both window dimensions, preserving aspect ratio.
- [x] Include Untangle and Soliterm; preserve actual pointer moves, keyboard ownership, undo, save and dirty-game protections.
- [x] Default embedded RDP/VNC to **Inherit pane size**. Retain **Fixed resolution** and saved dimensions when switching modes. Existing records without a mode inherit the pane, retaining their old fixed dimensions.
- [x] Use bounded physical-pixel sizes, even RDP widths, a 150 ms resize debounce and at least 500 ms between requests. The negotiated framebuffer remains the authority for rendering and input coordinates.
- [x] Negotiate RDP Display Control; retain the latest request through authentication and until server capabilities arrive. Recreate the dynamic channel when reopened.
- [x] Negotiate VNC ExtendedDesktopSize before sending SetDesktopSize; preserve screen identity/flags, coalesce requests and allow one outstanding request. Unsupported, multihead and view-only servers retain proportional local fitting. Rejected requests retain the existing framebuffer.
- [x] Enable TCP_NODELAY, reduce the RDP dirty-frame interval and active desktop UI polling from 50 to 16 ms, replace VNC's 10 ms input polling with notification wakeups, and coalesce only consecutive pointer movements without crossing click/key boundaries.
- [ ] Owner human checks and explicit merge approval.

These changes reduce known local batching/wakeup delays. They do not establish an end-to-end WAN latency benchmark or change the server's rendering speed. A server that cannot resize continues to fit proportionally; choosing pane inheritance cannot add remote protocol capabilities. View-only connections do not alter remote geometry. Installed external clients keep their existing resolution controls.

## Verification

Final local results: **25 UI/input/game tests**, **12 VNC protocol tests**, **2 shared input queue tests**, **1 session migration test** and **7 independent RDP/helper checks** passed. The final GUI passed isolated Wayland screenshot startup. Four focused helper tests passed separately. Formatting and approved-dialog scope checks passed.


Current-source scoped tests cover all thirteen boards at different viewports, real Soliterm pointer transfers and undo, Untangle drags, game keyboard isolation, puzzle variants, default/fixed size controls, resize scheduling and mouse-event ordering. Transport tests exercise VNC resize negotiation/denial/repaint and existing security/clipboard paths, bounded input releases and session serialization. The independent TLS/NLA RDP endpoint confirms a client request for 934 × 578 produces a rendered framebuffer of that size; frame/input, certificate-change and helper shutdown checks are retained. Actual RDP UI input and credential-vault tests also run against the owned endpoint. A regression discovered input loss during client-triggered server reactivation: the helper now holds the ordered input queue until reactivation completes. If a capable server ignores its layout request, input resumes after one second and further requests use local fitting until a later server reactivation. Endpoint tests preserve all 80 key events and verify the ignored-layout fallback.

The RFB protocol requires an incremental request after ExtendedDesktopSize; servers invalidate the old framebuffer and repaint after a size change. The regression checks that full replacement pixels arrive, without introducing a nonincremental request loop. [RFB protocol, ExtendedDesktopSize](https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst#extendeddesktopsize-pseudo-encoding).

Native Linux game previews were inspected at 1920 × 1080 and 960 × 680. The final local review contains both the current GUI and rebuilt helper, with notices and source/component identities in [the build manifest](games-desktop-review-build.json). It preserves the Wayland environment and passed isolated screenshot startup. This is local review evidence using matching cached GUI dependencies, not a new production package/provider qualification or Windows/macOS native acceptance.

## Run for human checks

Close the older app first, then run:

```bash
"/home/asad/.codex/worktrees/termcraft-issue-3/app/target/issue-4/run-terminal-row-review"
```

Check all games after resizing; compare fixed/inherited RDP and supported VNC sessions, pane changes, display scaling, typing, dragging, wheel input, focus loss and reconnection. Also check a server without resize support. No merge is authorized by these automated results.
