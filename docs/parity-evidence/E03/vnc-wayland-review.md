# E03 Wayland provider independent review

2026-11-25. **Accepted as a partial engineering candidate. No Critical or Important integration blocker found.** This is not native compositor, original TightVNC, or platform qualification. The coordinator checkpoint remains 42/60 (70%); strict full/original qualification remains 0.

Reviewed the 27 normalized candidate files in `target/agent-work/e03-wayland-provider/handoff/files`, their source patch, baseline/candidate/handoff hashes, dependency provenance and owner verification logs. No production source was changed by this review, and no host portal, capture, clipboard or input operation was performed. This review wrote only this evidence document. No broad build or GUI suite was launched.

## Independently checked integrity

- All 27 candidate SHA-256 values match the supplied payloads. All entries in `handoff-sha256.json` match their artifacts.
- All nine existing-file baseline hashes match MAIN; the 18 null baseline entries are absent from MAIN as expected. The supplied baseline aggregate is `9ab4fdd8eb35de470a8e32d68a723bfa7b89926be52bd09b86f7c9a3e44772cf`.
- Fresh `patch --dry-run -p1` against MAIN succeeds for all 27 files, with no rejected hunks. The fixture wrapper has mode 0755; preserve it during integration because plain patch application does not carry executable metadata.
- Root Cargo.lock is absent from the change. The root dependency change only pins the already used zbus 5.19.0. The optional helper has its own locked crate and exact PipeWire 0.10.1 bindings.

## Lifecycle and security assessment

`src/services/vnc/wayland.rs:28-36` resolves and seals the package-adjacent helper before opening the portal. `src/runtime/manifest.rs:112-132` uses the existing bounded native-image/hash/architecture verification and immutable executable snapshot rather than PATH resolution. `src/services/vnc/wayland/helper.rs:47-73` duplicates descriptors above its fixed protocol descriptors and executes that snapshot; only the owned portal remote and private IPC endpoint become descriptors 3/4. Missing helper/manifest/hash fails this source. No downloader, CDN, default PipeWire remote or host capture fallback was added.

`src/services/vnc/wayland/portal.rs` pins the portal's unique owner, registers the predicted Response match before the call, validates returned request/session handles, and owns each pending request/session. Signal queues, response deserialization, selected stream count and result dictionaries are bounded. Cancellation is checked during async waits; ordinary calls have bounded deadlines and permission waits have an explicit 120-second limit. Drop sends owned Request.Close and Session.Close concurrently under a 600ms cleanup deadline. Session Closed is permanently latched by `ensure_active`; stale input/capture stays rejected. Combined control sessions use RemoteDesktop Start and ScreenCast selection/remote opening, require keyboard and pointer grants, and never mix Notify with EIS.

The frame path validates format, dimensions, positive stride, chunk extent, single plane and pixel count before copying. Nontrivial crop/transform/corrupt metadata fails closed. The helper requests 2–8 memory buffers, uses DONT_RECONNECT and prefers the v6 serial target, drains old frames and keeps one latest frame. Parent IPC checks magic/dimensions/exact bounded body length before allocation; partial/cancelled bodies cannot publish. Worker/child failures propagate, live frames time out, and owned children are killed/reaped. The app generation cancellation closure reaches portal and IPC waits, and the supervisor capture owner joins before shutdown. No frame or clipboard file was introduced.

Input remains explicit and permission gated. Canonical key aliases retain their original press keysym for release; pointer coordinates are scaled into the stream logical space; held keys and button/wheel masks are bounded. Notify errors terminate this service generation, after which owned portal closure releases the session. Existing X11 remains the default and Wayland clipboard requests are rejected before portal access.

## Evidence assessed and its limits

The owner's matching, hashed logs record helper decoder tests (2 passed), component tests (8 passed/1 isolated ignore), current app VNC tests (24 passed/3 isolated ignores), selected packaging tests (2 passed), and the explicitly invoked production-module private-portal/native-PipeWire fixture (1 passed). These executions were inspected, not rerun by this reviewer. The fixture uses a private bus, private runtime/config, private daemon with no device monitors/session manager, and an owned Rust 64x48 RGBx source. DISPLAY/WAYLAND_DISPLAY are unset. It exercises actual helper frames plus mocked view-only/grants/denial/revocation/control/release/pending-Start cancellation. Final owned source termination/reap 143 is intentional; private daemon exit 0 is recorded. The prior videotestsrc abort remains visible in a separate red log rather than being converted into passing evidence.

ELF evidence shows no mandatory PipeWire DT_NEEDED in the app module-test ELF and libpipewire only in the optional helper. It does not replace final release-app ELF inspection. Packaging preserves the prior inputs while adding optional source/lock/examples, notices, native dependency inventory and explicit prebuilt provenance. Dependency provenance includes all metadata entries; the actual selected Linux notice traversal records 55 dependency crates. Provenance does not claim reproducibility of an arbitrary supplied prebuilt helper.

The primary portal documentation was checked during review: [ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html) specifies the fresh session lifecycle, v6 serial targeting, and compositor-space stream size; [RemoteDesktop](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html) specifies combined sessions, logical pointer coordinates, Notify signatures and the exclusive EIS/Notify choice. The implementation follows those boundaries. No claim is made about a real compositor accepting these calls.

## Integration conditions

Integrate only as the reviewed partial source candidate. Merge the shared `open_source` cancellation argument and `bind_cancel` hook with the accepted macOS/browser candidates rather than overwriting either candidate. Preserve all existing packager arguments and wrapper executable metadata. Re-run the coordinator's relevant integrated VNC/package checks and actual release ELF inspection after that combined integration.

Real portal consent/revocation, native compositor input, EIS, clipboard, fractional scaling, unsupported frame transformations/formats, resizing/multiple streams, general distribution ABI and original application parity remain unqualified. Keep E03 open and strict full/original qualification at 0. No scoped source fix is requested by this review.
