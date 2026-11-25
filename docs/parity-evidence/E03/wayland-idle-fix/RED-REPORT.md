# Genuine packaged GUI Wayland sharing: idle-stream RED

The requested real packaged-GUI acceptance is **not complete**. A genuine native idle-desktop failure was reproduced; no production MAIN changes were made.

## Actual release GUI evidence

Launched the genuine strict extracted archive 398d19fc6dbdf7d7dd5ee2a3a71ae8eda907bbbb31ad0117031c3a73fb106fe5 application, with binary SHA256 c8d09ba5fd2c36d567c6885f117d6068e2a87185648db883853cde84d678f7e6. Matrix asserts that hash before process launch. Normal real GUI, no provider driver or instrumentation: opened Servers, scrolled to VNC desktop sharing server, selected Wayland portal, entered memory-generated random eight-character password into volatile password field, enabled control/transport opt-ins and clicked actual Start.

The genuine GNOME native RemoteDesktop consent window appeared. Fixture-only direct native input enabled Allow Remote Interaction and clicked Share. Screenshot gui-vnc-running.png shows actual GUI Running, 127.0.0.1:5900 and Stop VNC server. running-state-listeners.txt/processes.txt show actual listener and sealed helper. Separate memory-only RFB 3.8 client selected VNC authentication, DES-encrypted the random challenge using password held only in memory, successfully authenticated and negotiated 1600×1000. Commands/14.ack records success and dimensions. No password was saved in commands, reports, source, logs, environment or app CLI arguments; password was cleared from UI when Start began. App Wayland wire logging was intentionally disabled, while target-only wire logging cannot see keys addressed to other windows. Product password-field undo/persistence protections were not bypassed.

After minimizing GUI to the owned static GTK target and idling, native screenshot helper exited, the real service listener disappeared and subsequent RFB input attempts failed BrokenPipe. App, target, compositor, portal and PipeWire remained alive; frontend log records real session closure. target-stable-listeners.txt has no5900; target-stable-processes.txt has app/target/compositor but no product helper. All explicitly owned children were reaped at fixture completion (cleanup.json).

**Capture artifact limitation:** first fixture accidentally saved the RFB capture and independent screenshot under the same filename. Thus the initial RFB capture acknowledgement proves a successful real protocol framebuffer response, but retained PNG is the independent monitor screenshot; it is not claimed as retained RFB pixel proof. Fixture filename handling is corrected for the final replay. The failed helper also made subsequent screenshot cache stale; those images are not treated as current GUI state. Final fixture now refuses stale screenshots after native helper exit.

## Independent deterministic native reproduction

idle-red/idle-red-result.json and idle-red-run.log independently establish:

1. Unchanged exact-source production provider opened genuine GNOME portal and captured valid800×600 frame.
2. Owned target remained static for **7.000331796seconds**.
3. The genuine optional helper SHA256 **26c3495d140afc8e90f318783af7a2a8b9484d69aa6e3c5c3c5e3c684096236e**, identical to the actual release helper, exited1. Production capture then returned `Selected portal video helper ended; check optional PipeWire helper/native ABI`.
4. Native compositor and owned target remained alive. Explicit Stop subsequently completed and children were reaped.

The run uses real signed Mutter/GNOME portal/private PipeWire, not mocked D-Bus or synthetic video. Exact provider source binary is the previously byte-verified unique private harness; this second run isolates helper behavior and is labelled separately from the real GUI evidence.

Root cause: engines/wayland/src/capture.rs::Capture::frame returns a timeout after five seconds without an increased buffer revision, even when it already has a valid frame and the worker/stream remain healthy. Mutter can publish only on actual damage; unchanged desktop pixels are valid idle video. Both actual product and independent native fixture use this exact helper, explaining their concurrent endings.

## Private TDD fix packet for review

`idle-fix.patch`, `idle-fix-hashes.json`, `idle-fix-source/src/capture.rs` contain the candidate. Production changes are two conditions plus a comment:

- Keep five-second deadline while **no initial frame** exists.
- After a valid frame, retain cached pixels while worker is alive and native error flag remains false.
- Explicitly reject cached pixels if the owned worker has exited, including cancellation, normal exit or panic; native core/stream error handling remains unchanged.

Four new tests exercise actual Capture::frame using owned live/exited worker threads, aged capture state, and cached frame/error state. RED log shows two expected failures (idle valid frame wrongly times out; exited worker wrongly returns cached video), two safety cases pass. GREEN complete engine library log shows six passed, zero failed (four added + two existing frame decoding/bounds tests). Every scoped cargo command used CARGO_BUILD_JOBS=2; unique private package/bin final-wayland-idle-helper-private, version0.1.0-idle-fix.1. No candidate binary or release was built before review.

Tradeoff: absence of new damage is not a liveness failure. Native core/stream errors, worker exit, outer portal session closure and normal application cancellation remain liveness signals. A completely silent hung transport that reports no failure cannot be distinguished from a healthy unchanged desktop by a buffer-age timer; the fix intentionally avoids the demonstrated false positive. Genuine native backend loss/session revocation and real GUI Stop must be rechecked after review/integration and a new genuine artifact, together with idle lasting beyond five seconds. This report does not claim those final acceptance cases pass yet.

Isolation reused signed private Mutter51.1/GNOME portal51.0-2 infrastructure, 1600×1000 virtual monitor, existing owned virtual keyboard before target map, private bus/home/config/data/runtime, network/PID namespaces, hidden host/home/run and minimal/dev without hardware/input. No host display, portal, clipboard, input, home, network or service access occurred during runs. Original release binary/helper were never replaced or modified.
