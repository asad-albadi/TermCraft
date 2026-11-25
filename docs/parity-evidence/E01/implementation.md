# Embedded Browser implementation evidence

The Linux Browser session runs installed Chromium inside a TermCraft workspace
tab. New Browser sessions select Embedded in TermCraft. Existing saved Browser
sessions without the new options retain the default system browser behavior.
The form offers both modes and an HTTP(S) proxy endpoint for the embedded mode.

## Implementation

- The Linux worker starts the direct Chromium payload from
  `/usr/lib/chromium/chromium`, with a mode-0700 temporary home/profile and
  private inherited descriptors for the NUL-delimited DevTools protocol.
  It uses Chromium's standard sandbox and TLS validation. Startup and close
  use the app-owned process group. Close cancels the worker, reaps the process,
  and removes the owned profile. A separate profile is created for each tab
  generation.
- The tab paints the latest Chromium-rendered frame as an egui texture. The
  app bounds messages, decoded dimensions, pending frames and command input.
  Navigation, authentication and screenshot capture run outside the UI thread.
  The tab offers address entry, back, forward, reload, stop, page title, URL and
  loading and error states.
- The view forwards pointer coordinates, scroll, editing keys, text and IME
  input. Copy and paste use explicit input events. Browser prompts run in the
  Browser view, outside terminal and macro input. Website and proxy
  authentication use a challenge-specific action with transient credentials.
- The address bar accepts HTTP, HTTPS and an empty page. Chromium handles
  web requests inside the owned process. The provider rejects unsupported
  document requests, closes pop-up targets, dismisses JavaScript dialogs,
  intercepts file selection and denies downloads. The in-tab notice reports
  these unsupported interactions.

## Local verification

- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --test browser_session`
  exercised four passing cases. The real engine produced fixture pixels,
  submitted exact UTF-8 input from a click, updated the URL after a redirect,
  and traversed back and forward. It also resized and rejected a local-file
  address. Separate loopback fixtures verified site authentication cancellation
  without an Authorization header, rejection of an untrusted HTTPS certificate
  before serving the page, and proxy authentication only after the caller's
  explicit action without a direct fallback. A serialization case checked the
  legacy launch mode and rejection of credential-bearing proxy configuration.
- `TERMCRAFT_REQUIRE_CHROMIUM=1 cargo test --offline --bin harbor-workspace --lib browser_`
  passed the Chromium workspace and process-ownership tests. The workspace
  test inspected the egui texture and painted mesh, used ordinary egui
  pointer and text events, and matched the server request. The startup
  cancellation test checked private directory mode, reaping and removal,
  with a second process and an unrelated profile sentinel intact.

The fixtures use app-owned loopback addresses without a public test website.
The egui test exercises app rendering and the browser provider without a
physical display. Native IME behavior and unsupported-page interaction
notices have not been measured in a desktop session.

## Qualification and remaining work

The measured Linux runtime was Arch Linux x86_64 Chromium 157.0.8103.38,
distribution package 157.0.8103.38-1. Its binary, archive, build recipe and
license measurements are in
[the browser manifest](../../../packaging/browser/manifest.json). The
[packaging contract](../../../packaging/browser/README.md) describes the
Linux closure and unqualified Windows and macOS build/runtime candidates.

The macOS shared Unix transport now has a [source-checked candidate](macos/README.md). A [Windows private-pipe/bootstrap source candidate](windows/README.md) is also present and type-checked. Both platforms' native GUI tests and packaged qualification remain open, as
does P01/P08 packaged Chromium runtime closure. The installed Linux engine
is available independently of packaging. The
[original behavior report](../original-scope/README.md) records the
unmeasured original Windows control semantics and engine defaults.

Primary interface references are the
[Chromium headless source](https://chromium.googlesource.com/chromium/src/+/HEAD/headless/),
[DevTools Page domain](https://chromedevtools.github.io/devtools-protocol/tot/Page/),
[DevTools Input domain](https://chromedevtools.github.io/devtools-protocol/tot/Input/),
[DevTools Fetch domain](https://chromedevtools.github.io/devtools-protocol/tot/Fetch/),
and [DevTools Target domain](https://chromedevtools.github.io/devtools-protocol/tot/Target/).
