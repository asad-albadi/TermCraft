# E03 VNC independent review

2026-11-24. Read-only source review of the private candidate at `target/agent-work/e03-vnc` and additive integration hooks at `target/agent-work/e03-vnc-integration-hooks`. Production sources were not edited. **Ready to merge: with fixes. E03 remains open.**

## Reviewed identity and evidence

- `vnc-private.patch` SHA-256 verified as `9f3c7929c830df4aad1f7b668b70df83eb37b763ce9ecdf71ac8d114c4c40eaf`.
- All 12 entries of `vnc-candidate-sha256.json` and all seven entries of `vnc-integration-hook-sha256.json` matched their private files. Read the adjacent base-hash manifest. Reviewed the shared-hook diffs against current production: VNC additions preserve current Cron/NFS branches, UI initialization and dependency versions; the private package version is absent from the integration manifest.
- Read implementation notes, targeted test output (14 library and four UI passes), isolated native wire/UI evidence and Windows component output. These are existing implementer executions, not newly reproduced independent executions. The wire evidence includes actual dynamic X11 pixels and mandatory independent vncdotool 1.3.0 interoperability.
- Reviewed config/API, complete RFB service, Linux X11 provider, Windows provider, actual Servers card/password helper, supervisor startup/client bounds/cancellation and focused tests. No application builds, broad test reruns, live X access or host framebuffer/input/clipboard operations were performed in this review.

## Strengths

The listener defaults to loopback, requires explicit classic-transport consent and a nonempty nontruncated password, and rejects clipboard opt-in before source creation. Challenge generation uses OS randomness and response comparison is constant-time. Versions 3.3/3.7/3.8 and version-specific authentication failure framing are consistent with the claimed subset. Pixel shifts, channel masks, frame sizes, encoding/clipboard lengths, client count, message deadlines and shared frame snapshots are bounded. Frame coordinate clipping avoids out-of-bounds reads.

Capture runs off the UI thread and uses the explicitly selected source. Network workers cancel independently of another service; stop drops the owned listener, joins workers and releases remote input without terminating the source display. The single-controller flag remains held until queued input is drained and release completes, preventing a replacement controller from inheriting pending input. View-only ignores input at the server boundary. Source capture failure stops the service. Linux authority reads and local connection/transport operations are bounded. Windows GDI resources are created and released within each capture call, and capture/input checks the active desktop. UI startup is asynchronous; password state uses zeroizing storage, clears after each Start attempt, clears editor undo history and is absent from ordinary workspace serialization.

## Issues

### Critical

None established.

### Important — preserve keysym case and required modifiers in remote input

**Locations:** private `src/services/vnc/x11.rs:204–213,322–335`; private `src/services/vnc/windows.rs:182–187`.

The X11 keyboard setup flattens all symbols from each keyboard row into `(keysym, keycode)`, losing the symbol's level/modifier requirement. Key handling subsequently injects only that keycode. On a normal US mapping, both `a` and `A` map to the same keycode; an RFB `A` press/release without a separate Shift press therefore enters lowercase `a`. Likewise `#` without Shift enters `3`. This is a real interoperability case for clients with a different keyboard layout and for clients that send character keysyms directly. The current native test's ordinary held-key event does not cover it.

The Windows candidate has the equivalent defect: `VkKeyScanW` returns a required shift-state byte, but `code as u16 & 0xff` discards it, so character keysyms such as `A` and `#` also lose their required modifiers. This is a source-level finding; no Windows execution is claimed.

[RFC 6143 §7.5.4](https://www.rfc-editor.org/rfc/rfc6143.html#section-7.5.4) explicitly explains both cross-layout `#` input and uppercase `A` arriving without Shift. The server must interpret the requested character case rather than relying solely on incoming Shift state. [Microsoft's VkKeyScanW reference](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-vkkeyscanw) documents the returned high byte's Shift/Ctrl/Alt requirements.

**Fix:** retain modifier/group information during translation and synthesize the required modifier transition while preserving real Control/Alt shortcuts and the controller's held modifier state. Track any synthesized presses for disconnect/stop release. Account for lock state and modifier restoration; simply keeping Shift pressed after a character would create another bug. For Windows, honor the translation's required modifier state or use a character-input path with explicit shortcut semantics.

**Focused verification:** extend the existing private isolated X11 fixture to send uppercase `A` and shifted punctuation without Shift events and assert the target window's interpreted characters, not just the raw keycode. Include explicit held Shift followed by lowercase input, a Control shortcut, and disconnect cleanup to verify restoration and ownership. Keep all live X work inside the existing private harness. Add a Windows mapping/component regression now and require native keyboard validation before claiming Windows control support. A full existing suite rerun is unnecessary until the fix changes shared behavior.

### Minor

None requiring action in this slice.

## Declined to judge / remaining scope

- Native Windows capture/control, secure-desktop transition timing, DPI/multi-monitor behavior and synchronous GDI cancellation remain unrun qualification work; component compilation does not prove them. This does not excuse the concrete translation defect above.
- Bidirectional clipboard, native macOS, approved Wayland portal capture/control, browser HTTP viewing and exact original TightVNC settings remain explicitly missing scope, so this review does not mark them implemented or close E03.
- Full integrated Cron/NFS/VNC build and regressions remain root integration work. The additive hooks were inspected, not compiled here.

## Assessment

The bounded protocol/capture/ownership structure and real private native evidence support continuing integration after the Important input translation issue is fixed and narrowly verified. No other Critical or Important protocol, lifetime, listener ownership, password/UI persistence or view-only bypass issue was established in the inspected candidate. The result supports neither a native Windows runtime claim nor overall E03 closure.
