# B06 generic idle foundation independent review

Review date: 2026-11-25. Scope: the eight files in `target/agent-work/b06/reports/owned-files.json`, against the current MAIN base. No game catalog/rule implementation or B06 completion is accepted by this review. No implementation files were changed and no Git operation was used.

**Disposition: request changes for the generic foundation before integration. Two Important lifecycle findings follow; no Critical finding was established. B06 remains incomplete regardless of their resolution.**

## Important findings

### 1. Canceled touch or lost pointer release can keep every viewport blocked and repaint continuously

Private `src/ui/games.rs:68-81` sets a global wake owner after a pointer-button wake and only clears it when that same viewport reports no held keys or pointer buttons. It does not handle touch cancellation, pointer/focus loss, or disappearance of the owner. While it remains set, every viewport clears all events and immediately requests another repaint.

A concrete supported-backend sequence is: the saver is active; a native touch starts, generating a primary pointer press and dismissing the saver; that touch is canceled by the OS rather than ended. The pinned `egui-winit-0.36.2/src/lib.rs:930` cancellation branch emits `Touch(Cancel)` and `PointerGone`, without a pointer-button release. In `egui-0.36.2/src/input_state/mod.rs:1203`, `PointerGone` explicitly preserves button-down state. `WindowFocused(false)` clears keys/modifiers at input-state lines 440-447, but does not reset the pointer-down state. Consequently the idle gate keeps seeing `pointer.any_down()`, consumes all subsequent key/text frames (including attempts to use another viewport), and schedules immediate repaints until a fresh pointer press/release happens in the owning viewport. Ordinary keyboard use cannot recover the app. An owner that stops receiving callbacks has the analogous absence of a release/recovery path.

This is a code/dependency-trace finding, not a claimed native-window reproduction. The five supplied UI tests do not exercise canceled touch, lost pointer release, owner disappearance, or multi-viewport ownership recovery. Add bounded regression coverage for those state transitions, and explicit safe cancellation/lifecycle recovery that does not forward the wake input or let an unrelated idle viewport clear an active held-input guard. Use bounded repaint scheduling while waiting for input instead of an unconditional immediate repaint loop.

### 2. Detached native close requests are skipped by the idle gate

Private `src/ui/windows.rs:207` returns from the detached viewport callback before its existing `close_requested()` handling at lines 210-213. A native close event is a viewport event, not one of the input events recognized by `screensaver_frame`, so clicking the title-bar close control while idle neither dismisses the saver nor reaches `CancelClose` / `window_close_prompt`. The callback continues creating the detached viewport; the user's existing reattach/close confirmation flow is bypassed until they first wake the screen and request close again. This also affects an outstanding held-input wake guard.

Move lifecycle interception ahead of the content/input gate (with an explicit policy for showing its confirmation after dismissal), or otherwise preserve the existing close flow without forwarding terminal input. Validate a real non-embedded viewport close event; the current detached test deliberately uses the embedded fallback and cannot establish native close behavior. No process destruction or native-window closure was inferred from this finding.

## Accepted portions and evidence

- Independently recomputed all eight private source SHA256 values and all eight current live-base/absence checks: all match the manifest. `patch --dry-run --batch -p1 -i target/agent-work/b06/reports/foundation.patch` succeeds for all eight paths against MAIN. Patch SHA256: `d90a97a03b4cf40dd8eb7b7ad2f12e099998eb66bf47626027a4037f9ec18d7f`.
- The patch excludes Cargo.toml, Cargo.lock, private package version `0.1.906`, and binary name `harbor-workspace-b06`. The three modified existing files are narrow hooks. Compose `src/ui.rs` and `src/ui/windows.rs` with pending VNC/Wayland work; do not replace MAIN wholesale.
- Re-ran existing built executable `target/debug/deps/harbor_workspace_b06-584f86ae6f088c6a ui::games::tests --nocapture`: 5 passed, 0 failed, 153 filtered out (1.43 s). Re-ran `target/debug/deps/games-0815237075421891 --nocapture`: 2 passed, 0 failed. No Cargo invocation, new compilation, broad suite, host input, or clipboard access was performed by this reviewer.
- Inspected actual RED evidence: `red.log` reports absent `games` API E0433, `ui-red.log` reports absent methods/state E0599/E0609, `repeat-red.log` and `input-red.log` report actual behavioral assertion failures. Final GREEN logs show 2 policy and 5 UI passes. `fmt.log` is empty and the owner reports its exit status as 0; this review did not independently rerun formatting.
- Policy is Duration based, saturating, default OFF, explicitly enabled and session-only. No preferences/JSON schema fields are added, so legacy/default settings remain unchanged. The 30–3600 second UI timeout is provisional new optional behavior, not a measured original setting.
- Root render calls terminal/network/background polling before the idle gate. App logic also pumps background state. The gate does not delete queued PTY bytes, processes, sessions, or output. Root and detached raw-PTY tests demonstrate pre-idle bytes retained, stable process identity, and subsequent communication. They intentionally consume wake text; they do not prove wake text delivery or every network protocol under idle.
- Root gate precedes hidden widgets and shortcut/terminal/broadcast routing; detached gate likewise precedes its terminal/shortcut routing. Repeat Enter through release, a direct IME commit, and raw mouse motion are covered. Keyboard focus-loss recovery is partly supplied by egui clearing keys; pointer cancellation is the unresolved difference identified above. Multi-event IME composition, native focus transitions, and multi-viewport held-input ownership are not established by the current tests.
- Product text is nontechnical: unavailable games, optional idle-screen settings and a wake instruction. The existing independent penguin mark is reused. No original art, substitute game, invented GameRegistry, or game-specific rules are introduced.

## Evidence boundaries and remaining B06 work

The approved `docs/superpowers/plans/2026-11-23-08-services-platform.md` Task B06 requires the exact original Personal 26.6 exposed game inventory and measured rule/input/outcome fixtures. Those are still unresolved. `docs/parity-evidence/original-runtime/B06.json` explicitly marks partial emulated observation. The actual Wine log contains `RtlpWaitForCriticalSection` loader-section timeouts (lines 82-83); archive entry names are not an exposed Games catalog.

The root-authorized provisional policy consumes the full dismissal frame and held wake press/repeat/release, so users may need to retype the wake keystroke. Exact original wake behavior and native Windows screensaver behavior remain unmeasured. The detached PTY test runs an embedded headless callback, not an OS-window integration gate. These are explicit evidence limits, not reasons to claim equivalence or advance B06, E04/V07, a completion count, or the strict checkpoint.

No full Rust suite, native window gate, native Windows original qualification, game-rule fixture, or full network idle sweep is claimed. All review commands finished before this report was saved.
