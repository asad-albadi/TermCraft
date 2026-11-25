# X03 Linux graphics qualification — private review handoff

Private source: `target/agent-work/x03`, version `0.1.0-x03-private`. No MAIN files changed. Existing shared hooks still match original recorded base hashes. Apply only `x03-integration.patch` after fresh review; no Cargo/lock replacement. Existing direct libloading/image/base64 dependencies are reused.

Production starts a disposable capability subprocess on the fresh owned display before releasing any client. DISPLAY and XAUTHORITY are child-scoped; UI process never loads Xlib/GL or changes global environment. Probe cancellation/display exit/eight-second deadline kills and waits the child. Crashed/invalid/timed-out probes cause the fresh server to be discarded, because interrupted native graphics cannot promise RandR restoration. Bounded worker channel is three slots for Capabilities/Ready/Ended, permitting shutdown without rendering. Existing Windowed/Fullscreen controls remain; compositor chooses monitor. No indirect-GLX enablement or access-control relaxation.

Native GLX creates actual context/window, renders four quadrants, glReadPixels checks all4096 pixels and stores actual PNG. Direct context does not imply GPU: renderer/software classification shown explicitly. Composite manually redirects a green window, obtains its offscreen pixmap, composes onto a blue output and independently reads samples. RandR queries an alternate actual advertised screen size, requests the transition, observes geometry plus screen-change event, and restores/checks initial geometry before clients are released. UI shows versions/render result/errors without equating advertised extension with working rendering.

## Final evidence

- `final-live.log`: **7 passed**, including no false advertisement/render success, actual capabilities, PNG independently decoded/color-checked, RandR restored independently through x11rb, cancelled display remains usable, wrong-cookie rejected, and stalled child cancelled/reaped.
- `final-ui-units.log`: **4 passed, 4 opt-in ignored** (one subprocess test also independently reports1pass).
- `final-ui-live.log`: **4 passed**: crashed probe discards authority/server before client release; embedded SSH config starts only requested forwarding; owned local shell child receives scoped environment while parent unchanged; UI-owned engine starts/stops and removes authority.
- `native-red.log` / `first-native-green.log`: previous genuine SIGSEGV was library lifetime: Xlib retains extension close-display callbacks. GL/Composite/RandR handles now remain alive through XCloseDisplay.
- `ui-fixture-probe-red.log`: final opt-in fixture exposed one previously unconverted test using test executable as probe. It now explicitly uses fixed qualified app, like other UI fixtures. Final4pass above.
- `capabilities.json`, `gl-scene.png`: actual GLX1.4, Mesa llvmpipe LLVM23.1.4 OpenGL4.6,4096/4096; Composite0.4 actual manual composition; RandR1.6 1280×720→1152×864 with event and verified initial restoration. PNG rows retain glReadPixels bottom-up order, documented in schema.

All actual graphics/UI tests executed ONLY through existing `tests/support/run_xserver_isolated.sh`: private bwrap network/PID/tmp/X socket namespace and nested Cage. Fixture uses extracted qualified c6f12002… Xwayland24.1.15 engine. Fixed private app/test copies avoid concurrent MAIN shared-target overwrites; hashes recorded. All command sessions completed. Cage shutdown Broken pipe is fixture teardown after passing tests. No host display probe/lifecycle operation was run.

## Reproduction and limits

Use `CARGO_BUILD_JOBS=2`, shared warm CARGO_TARGET_DIR, private unique package. Coordinate app-producing integration builds since normal debug app path is shared. Copy final app to `qualification/x03-probe-app`, test to qualification/x11_extensions, hashed UI test to qualification/x03-ui-tests. Set TERMCRAFT_TEST_X11_PROBE to absolute copied app. Run existing isolated script with extracted package root and copied tests (`--include-ignored --nocapture --test-threads=1`); UI copied test filter `ui::xserver::tests --ignored --nocapture --test-threads=1`.

Qualified Linux/Arch ABI/software-rendering slice only. Native Windows/macOS graphics providers, GPU hardware rendering, alpha compositor behavior, exact original MobaXterm behavior and cross-monitor semantics remain unverified. Required native runtime libraries/Arch packages are explicit in packaging/x11/manifest.json and missing libraries become capability errors. No bundled dependency closure claim. X03 must remain scoped/partial until parent reconciles whole requirement and review; no checkpoint self-advancement.
