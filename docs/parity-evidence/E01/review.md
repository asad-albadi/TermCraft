# E01 independent review

Two important correctness findings remain in the Linux UI. These findings are from source inspection. The regression checks below have not been run.

1. **The address field can keep showing the previous origin after focus leaves it.** In `src/ui/web_browser.rs:97–102`, a redirect while the address field has focus updates `current_url` but skips updating the displayed field. On blur the two tracked URLs already compare equal, so subsequent repaints never restore the committed address. The old origin can remain above an interactive page from a different origin. Preserve the draft during editing, then reconcile the displayed address on blur or cancellation, accounting for a simultaneous Go/Enter action. Regression check: hold a redirect between two loopback fixture origins, focus the address field, release the redirect, then click the page. Assert that the displayed origin matches the final page without another navigation.

2. **A saturated input queue can drop a mouse release and leave Chromium dragging.** `src/ui/web_browser.rs:290–295` clears local capture before attempting delivery at lines 350–352. `src/browser/mod.rs:214–217` rejects delivery when the 128-entry queue fills, while `src/browser/engine.rs:502–510` clears Chromium's pressed-button bit only on a delivered release. Later moves can therefore reach the page with the left button held after the physical release. Subsequent successful sends also erase the feedback at `src/ui/web_browser.rs:39–41`. Preserve releases until delivered, coalescing intervening motion where appropriate. Regression check: in an input-queue test, accept one press, keep the consumer paused while filling the queue with moves, release, then resume consumption and move. Assert one release precedes the final move and its button mask is zero.

The supplied passing Linux fixture results do not exercise either transition. Windows/macOS qualification and packaged runtime closure remain open as recorded in the implementation report.

## Follow-up verification

Both regressions reproduced against the reviewed source with installed
Chromium. After a redirect between two loopback origins, the edited address
remained visible after focus left the field. With the input queue saturated by
pointer motion, the page did not receive the outside release before later
motion.

The field now preserves an unsubmitted draft during editing and restores the
worker's current URL when editing ends. The canvas tracks an accepted press
until its release enters the queue. UI updates retry a pending release before
forwarding later pointer input. Focus or pointer loss also requests release.

On the corrected source, the redirected page's actual address appeared after
blur without another navigation. The pointer fixture observed one press, one
release, and a zero button mask on the following move. The saturation regression
passed three additional direct reruns.

- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --bin harbor-workspace embedded_browser_` passed all three Chromium egui cases.
- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --test browser_session` passed all four provider and settings cases.
- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --lib browser_startup_cancel` passed the process and profile cleanup case.

## Resize after history restoration

The serial browser fixture reproduced a resize that acknowledged 300 × 200
but continued to deliver decoded 640 × 480 frames. The page title and loading
state confirmed that forward navigation had completed.

An independent loopback probe issued a surface capture at 640 × 480, then
requested a 300 × 200 emulation resize before the capture replied. Chromium
acknowledged the resize first on all eight runs. After the capture replied,
`Page.getLayoutMetrics` reported 640 × 480 on all eight runs. This matches
Chromium's screenshot completion code, which restores the prior widget size
and emulation parameters. Repeating the same command ordering with
`Browser.setContentsSize` and no device emulation retained 300 × 200 on all
eight runs. [Chromium PageHandler](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/content/browser/devtools/protocol/page_handler.cc).

The provider now resizes the owned headless browser window. It discards a
pending frame on a size change and ignores a capture requested at the prior
size. The fixture confirms that the forward document came from the
back-forward cache through `pageshow.persisted`. It checks two actual JPEG
sizes, the painted pixels of a button anchored to the bottom-right corner,
and clicks that report the exact DOM layout sizes of 300 × 200 and 460 × 320.
The launch fixes the device scale factor at one. The fixture checks that the
page reports DPR 1, so the decoded frame and click coordinates share units.

- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --test browser_session -- --test-threads=1` passed all four cases. The BFCache and DPR regression then passed three additional serial reruns.
- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --bin harbor-workspace --lib browser_` passed the cleanup case and all four matched UI cases.

These results qualify the resize behavior on the existing Arch Linux x86_64
runtime. Native macOS browser execution and display scaling remain unmeasured.
