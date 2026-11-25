# Embedded Browser implementation contract

Follow-on to approved E01 in `docs/superpowers/plans/2026-11-23-09-scope-evidence.md`. Scope is established by the exact target and official history in `../original-scope/README.md`; this is distinct from the SSH file pane. Native/original default engine and precise controls remain unmeasured.

Use a maintained Chromium engine with an app-owned profile and private process communication, rendered inside the Browser tab. The existing default-browser action remains a clearly labeled compatibility choice. Linux can qualify the installed engine initially, but a system launcher alone is not the embedded feature or the packaged-runtime completion. Introduce pinned package metadata and native Windows/macOS build/runtime contracts through P01/P08. Do not disable Chromium's sandbox or certificate validation to make a fixture pass.

The provider owns a private anonymous debugging pipe, process, profile and worker. Do not expose an unauthenticated debugging TCP endpoint. Commands/events are bounded, carry runtime tab/connection identity, and run outside the UI thread. Limit frame dimensions/pixels and encoded protocol messages before decoding; keep only the latest pending frame. Start/close/resize/navigation/input remain cancellable. Kill and reap only the owned process group. A startup timeout must clean its temporary profile without touching ordinary browser profiles.

Deliver in-tab address entry, back/forward/reload/stop, title/current URL and progress/error state, keyboard/text/IME and pointer/scroll input, clipboard by explicit user action, private HTTP auth prompts and visible TLS failures. Proxy settings contain an endpoint, with transient credentials outside ordinary JSON. Limit initial navigation to HTTP(S) and an empty page; block implicit local file execution/external protocol invocation. Popups, dialogs, file selection/downloads need explicit app handling or clear unsupported status. Never dump full protocol payloads or credential-bearing URLs to logs. Page trust/auth controls cannot be captured by macros.

Test against app-owned HTTP/HTTPS fixtures: a page actually renders into the egui tab; typed text and clicks submit exact values to the fixture; navigation history and redirects update the correct tab; TLS errors fail closed; auth cancellation sends no credentials; a proxy fixture receives only configured requests; close/cancel leaves no owned browser; unrelated user browser and profile remain untouched. Use the real maintained engine for these cases, with fixture traffic isolated where possible. Tests must not substitute fake HTML/synthetic screenshots for browser execution.

Start with a coherent provider module and targeted RED/GREEN fixtures before integrating common UI files. Capture dependencies, license/source/version/hash and remaining native/original observations in the report. E01 stays implementation remaining until the real browser workflow and its integrated tests pass review and the full suite.

Primary API references checked 2026-11-24:
- https://developer.chrome.com/docs/automation-and-testing/headless
- https://chromedevtools.github.io/devtools-protocol/tot/Page/
- https://chromedevtools.github.io/devtools-protocol/tot/Input/
- https://chromedevtools.github.io/devtools-protocol/tot/Target/

These references describe the engine and protocol; the private-pipe rendering architecture is a TermCraft implementation choice, not an original-app observation.
