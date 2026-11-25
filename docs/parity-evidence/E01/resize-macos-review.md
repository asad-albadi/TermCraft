# BFCache resize and macOS candidate review

**Important — pointer presses can hit a different control during a resize.** In `src/ui/web_browser.rs:255–271`, the UI queues Resize, stretches the previous texture across the new canvas, and continues forwarding input. At `src/ui/web_browser.rs:338–346` the press is scaled using the previous frame dimensions. The worker applies the resize synchronously before processing that press (`src/browser/engine.rs:398–411,525`). Clearing the worker's pending frame leaves the previously uploaded egui texture intact.

For a deterministic regression, display a 300×200 fixture with adjacent controls on opposite sides of CSS x=200. In one egui update enlarge the canvas to 460×320 and press at x=290 relative to the canvas, before rendering a replacement frame. Input currently reaches the resized page near CSS x=189. Assert that fresh presses wait for a frame corresponding to the accepted resize (or are discarded), then assert a press at the same point after that frame reaches x=290. Preserve release delivery for previously accepted presses. The reported settled-frame and BFCache tests address a different timing interval.

No additional important issue was identified in the scoped macOS container checks, application lookup, or shared descriptor setup. The architecture check tests eligibility only. Native signing, startup, teardown and rendering on macOS remain unverified. This review inspected source and recorded test evidence without rerunning the platform checks.

## Viewport receipt fix

The real egui regression reproduced the old transition. After the viewport
grew to 461 CSS pixels, a press over the right control was sent at CSS x=217
and activated the left control. With the new frame painted, the same gesture
reached the right control at CSS x=289.

Each accepted resize now has an increasing revision. The browser worker tags
a capture with its requested revision and dimensions, and drops replies
from earlier revisions and mismatched image sizes. Pointer presses, motion
and wheel input are eligible after a matching texture has been painted in an
earlier frame of the current egui window. A return to the original dimensions
still requires the new revision.

The view clears a held pointer before sending the resize, retrying if the
input queue is full. A synthetic mouse-up can still generate a click when a
page holds pointer capture. The worker uses Chromium's no-button move for a
release outside the page, which clears the button mask and releases pointer
capture without generating a click. Busy feedback uses the existing status row
so the feedback itself does not resize the pane while a release is pending.
[Chromium pointer capture handling](https://chromium.googlesource.com/chromium/src/+/a6c1687b03092871b0486030ccc18f2fbb27dad0/third_party/blink/renderer/core/input/pointer_event_manager.cc).

The loopback fixture exercises the stale press, a settled click, an explicit
DOM pointer capture across a resize, and a large-to-small-to-large cycle
without painting the intermediate viewport. It observes capture loss with
zero held buttons and verifies that only the settled clicks reached a control.

- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --bin harbor-workspace --lib browser_` passed five UI cases and the process/profile cleanup case.
- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --test browser_session -- --test-threads=1` passed all four provider cases.
- The pointer/receipt regression passed two additional direct reruns. The BFCache resize and DPR regression passed three additional serial reruns.

## Independent re-review

**Closed.** The UI now accepts a fresh press only after a matching resize revision has been painted in an earlier egui frame. Returning to the same dimensions cannot reuse the earlier paint. The worker discards superseded captures and checks decoded dimensions before publishing a frame. A held pointer is cancelled before resize enters the command queue, with retries under backpressure. The adjacent-control fixture covers the original wrong-target press and verifies capture loss without an extra click. No new important issue was identified in this fix. This re-review inspected the source and recorded test results without rerunning them.
