# E03 macOS VNC FIX1 independent rereview

2026-11-25. Scope restricted to the two findings in `vnc-macos-review.md` and breakage introduced by their private fixes. No implementation edits, native actions, permissions, broad builds or Cargo invocation. MAIN VNC production source remains unchanged.

**Accepted for the two reviewed fixes. No remaining Critical/Important finding within this fix scope.** This is approval of the private correction, not native qualification, a standalone application compile claim or E03 closure.

## Artifact identity and checks

- Candidate manifest SHA-256: `83d281c823e192e7d0b4a2be94ee71d5d50cac17672368c0480b8ab091b2aa0c`.
- Full integration patch: `d1575be0ca6487c065e2c20c08b9026cfa9e04a9c3a7b53eeda7524b8f7b0c2e`.
- Targeted fix patch: `836a38e4becfe4fc16650c472b4678540eb935cdb84ff7c0ec95930e1734f0c9`.
- Independently verified all 50 manifest entries. Applied targeted patch to a temporary copy of frozen round-zero macos.rs: resulting macos.rs and new macos_pointer.rs are byte-identical to candidate. Full integration patch independently passes structural dry run against current MAIN.
- Independently reran the existing compiled pure-proof executable: **11 passed, zero failed**. Inspected the hashed RED/green logs, latest aarch64 component check/LLVM logs and pool/linkage audits; these build logs were not represented as a new reviewer build.

## Finding 1 — center dragging corrected

The native mouse wrapper now accepts a single planner Event containing both event kind and button. Native movement, down/up and release all consume that exact pair. Owned center state maps to OtherMouseDragged 27 / Center 2, matching center down/up 25/26 / 2. Local-held-button avoidance and recording only successful remote downs remain in the caller; movement still uses owned state. The planner retains left/right precedence for multiple owned buttons and ignores scroll bits for drag selection.

Compared every used numeric event/button value against pinned objc2-core-graphics 0.3.2 generated CGEventTypes bindings. The new center sequence regression exercises the same planner consumed by native calls; recorded RED identifies old button 0, GREEN uses 2. Independent 11-test rerun includes center, left/right, multibutton precedence, no-drag scroll, and existing lifecycle checks. Native application middle-drag behavior remains unexecuted.

## Finding 2 — bounded Cocoa pools supplied

Native open, screenshot initiation, geometry, key/mouse, release, capture, input and clipboard operations now establish per-operation autorelease pools. Both native completion bodies establish their own pools, including error and late-callback exits. Drop delegates to pooled release. The source still stores only Rust-owned/plain state.

Reviewed pool boundaries and callback ownership: incoming raw content/image pointers are used only inside their callback invocation; display array and local retained objects drop inside the content pool; pixel conversion produces owned Rust Frame bytes before leaving the image pool; clipboard produces owned zeroizing Rust text before leaving its pool. The asynchronous image block holds explicit retained filter/config objects in CaptureObjects. Its destructor takes/drops both Option fields within a fresh pool, covering native disposal on a different queue and late disposal after cancellation. Automatic field destruction then sees None. No borrowed autoreleased reference escapes a pool.

Permit/channel/alive captures remain Rust-owned. Wrapping work in pools does not introduce a raw Rust Source pointer or release the global permit early; the native-copied blocks retain it until their destruction. Cancellation, bounded channel, timeout and one-inflight semantics remain intact. Nested operation pools drain independently rather than waiting for the full service lifetime. Native pool-drain behavior and memory growth have not been measured.

## Unchanged qualification limits

Wayland/shared-supervisor cancel argument remains an integration dependency; the patch alone is not claimed to compile as the whole application. Preserve X11/Windows behavior when combining patches. macOS14+ / maximum 5fps initial scope remains accepted. macOS12/13 capture remains unsupported. Real native frames, permissions/revocation, clipboard/input, callback Start/Stop races, pool memory behavior, hardware/Retina/hotplug, performance, final Mach-O dependencies and older-supported-macOS ordinary application startup remain UNAVAILABLE. Dynamic framework/source/LLVM checks are not final link/startup proof. This rereview closes only the two source findings; E03 and strict native parity remain open. Root reports global progress 42/60 (70%); this review adds no parity credit.
