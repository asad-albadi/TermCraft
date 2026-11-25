# Independent real local GNOME RemoteDesktop review

Date: 2026-11-25. Reviewed application HEAD `0de3e0d097b3e782bb0c05522bf304fa19d95d14`, and the sealed `target/agent-work/real-wayland-rd` evidence. No production source was edited. No native matrix or Docker matrix was rerun.

**SPEC: ACCEPT. QUALITY: ACCEPT. EVIDENCE: ACCEPT**, for the stated configured private native headless Linux GNOME/Mutter platform with an existing keyboard. No blocking issue found in this qualification. This verdict does not establish release artifact closure or full application parity.

## Fresh verification

- Reran `verify.py`: PASS, 68 checks, zero failures. Output: `fresh-verifier.log`.
- Independently hashed all 1,179 files in `EVIDENCE-SHA256.json`: zero mismatches, both before and after the verifier. The verifier rewrites its result/pixel metadata, but produced identical sealed bytes. Output: `seal-validation.json`.
- Reverified all four detached Arch package signatures against the pinned, previously provisioned binary keyring: exit 0 for each. Output: `fresh-signatures.json`. This review verifies against that existing trust input; it does not repeat its out-of-band keyring bootstrap.
- Decompressed all four package archives and compared every non-metadata regular file to the private extracted native tree: 456 exact byte matches. Output: `native-extraction-validation.json`.
- Repeated the scoped offline normal binary build using the declared manifest and `CARGO_BUILD_JOBS=2`: exit 0, cached completion in 0.07 seconds. The resulting binary equals the sealed package provider byte for byte. Its Cargo depfile lists the private `src/main.rs`, all five copied production Rust files, and copied embedded runtime manifest. Cargo fingerprint has no features and no extra rustflags. The build is the normal binary target, not a test executable.
- Added stricter independent event/request timing checks and exact raw keyboard sequence checking: PASS. Output: `independent-ordering.json`.

## Source linkage and native backend

The six copied production inputs compare byte for byte to the current application: `wayland.rs`, `wayland/portal.rs`, `wayland/helper.rs`, `keyboard.rs`, `runtime/manifest.rs`, and `packaging/runtime-manifest.json`. The standard harness directly includes these production modules and dispatches commands through the production `Source` methods. It supplies the surrounding Frame/Input/Source types and a cancellation marker; it does not replace the portal or implement its own product input injection. The test fixture module is behind `cfg(test)` and is absent from this normal build.

The actual signed private packages are Mutter 51.1-1, xdg-desktop-portal-gnome 51.0-2, gnome-desktop-4 1:51.0-1, and startup-notification 0.12-9. Native frontend/PipeWire/WirePlumber/Mesa/GTK/libei/Python package versions are recorded in `inputs.json`. Logs and process inventory identify the real Mutter default plugin, real GNOME backend, system portal frontend and private PipeWire/WirePlumber. Mutter reports a surfaceless software renderer without GPU. This establishes a real native headless compositor/backend path, not GNOME Shell or a physical GPU session.

Production control opens the RemoteDesktop API, requests keyboard and pointer devices, checks the granted device mask, and uses actual portal `NotifyKeyboardKeysym`, `NotifyPointerMotionAbsolute`, and `NotifyPointerButton`. View-only uses ScreenCast. Actual native consent screenshots, backend/frontend logs, stream nodes and receiver wire events support those paths. Failures are not converted into successful capture/input assertions.

The private package has an explicitly generated `PACKAGE-MANIFEST.json`: Linux/x86_64, `rdp_helper_version: unused`, `wayland_helper_version: 0.1.0`, and helper SHA-256 `26c3495d140afc8e90f318783af7a2a8b9484d69aa6e3c5c3c5e3c684096236e`. That checksum independently matches the supplied prebuilt `termcraft-wayland-engine`. Production resolves this package manifest, verifies the executable, seals its bytes into a memfd and launches that descriptor. The helper links native libpipewire-0.3. This exercise validates the supplied optional helper through the unchanged verifier/launcher; it does not rebuild that helper from current source or qualify a final packaged release.

## First-key attribution and fixture boundaries

The unprimed direct-native comparison is material evidence, not a waived product failure. With no product provider running, the target first receives `wl_seat.capabilities(0)`. After the first direct Mutter `a` down, it receives `capabilities(2)`, binds its keyboard, receives keymap and focus enter, then receives only that key-up followed by the second complete down/up. `direct-inputs.jsonl`, `owned-target.log` and GTK events preserve this sequence. The direct comparison script never launches the production provider.

The authoritative matrix explicitly creates a separate fixture-owned virtual keyboard before mapping the target. Its neutral Shift down/up is a setup operation on the privately owned compositor. Its timestamp 7092.315289741 precedes target mapping at 7092.652737467. Keyboard readiness at 7095.238751102 includes capabilities(2), get_keyboard, keymap and enter; it precedes provider capture and first product input. This models an already-present keyboard and is not a production priming workaround. No product source was altered to introduce priming.

The final raw receiver key sequence is exactly Linux keycodes 30 down/up, 42 down/up and 48 down/up: ordinary `a`, Shift and `b`. Every corresponding GTK event falls inside its precise provider command/ack interval, including the very first `a` down at 7099.615800139. There is no prior target key-down or extra injected key in the final trace. The fixture consent action log has no entries during the measured control command sequence. In `matrix.py`, direct fixture pointer calls are confined to owned consent controls; direct keyboard calls occur only in pre-target setup. Measured input commands reach production Source methods. This is sufficient attribution for this source harness, although it is not an independent bus-level packet recorder.

The limitation remains real: first injected-key delivery to a client on a completely keyboard-less, unbound seat is not qualified. The retained counterexample must remain attached to any summary of this result.

## Capture, lifecycle and isolation

Both control and view-only produce actual 800×600 RGBA target captures exactly equal to a separate native Mutter monitor-capture session, with every-channel mean difference zero. Known target center/edge colors match independently specified drawing code, and visual inspection shows the expected title and target content. The reference uses a separate native capture session but the same helper decoder; this is not fully independent decoder validation, which the handoff correctly acknowledges.

The provider delivers absolute pointer coordinates (350,300) as client coordinates (350,261), accounting for the 39-pixel title bar. Native button press/release telemetry is present. Source::Release releases held Shift and left button, and Source drop (the harness Stop operation) releases held `b` and left button. Strict timing puts key releases inside their command intervals; the verifier also ties button release pairs to those intervals. Both operations finish in about 51 ms, below the one-second bound. This is the source lifecycle, not proof of the full application's release GUI Stop wiring.

View-only key and pointer attempts return the expected error and leave target telemetry byte-identical. Native no-interaction Share fails closed in 1,146 ms, native Cancel in 1,132 ms, and cancellation during the displayed consent prompt in 839 ms. All are under 2.5 seconds. Frontend logs close all five product sessions. Provider processes exit normally; no sealed production helper survives. The one helper in the pre-teardown inventory belongs to the explicitly identified fixture reference capture. All explicitly tracked fixture children have wait results; private autoactivated descendants are confined to the PID namespace and removed at namespace exit. Target and compositor survive the product lifecycle operations.

The runner clears host display/socket/bus settings, hides host home and /run, uses isolated /tmp/home/config/data/runtime and session bus, unshares PID/network namespaces, and exposes no dri/input/snd devices. Read-only application/cache exposure and writable evidence exposure are deliberate. The native portal can enumerate /sys USB metadata because the broader host filesystem remains read-only visible; this is not whole-host metadata isolation. There is no evidence of host screen/input/device capture, host desktop bus use, or host-home capture. Expected missing-system-service/accessibility/FUSE/locale warnings coexist with successful native results and are not silently treated as successful functionality.

## Bounds of acceptance

Accept only the configured native headless Linux source-provider qualification, using the explicitly supplied prebuilt optional helper and the pinned native environment. Physical keyboard/GPU desktops, hardware encoding, GNOME Shell release sessions, the complete application release GUI and package installation/upgrade, Windows, macOS, and the original application's behavior remain outside this result. No broad native-platform matrix or final Arch bundle qualification is inferred. Artifact closure must be established separately by the owning task.
