# Desktop foundation review and fix pass

2026-11-24. Independent read-only desktop foundation review reported three Important findings and no Critical finding. One implementation fix pass; no repeated reviewer loop.

1. A full input queue could discard focus release and leave keys/buttons held. Release now has an independent atomic epoch/barrier; obsolete queued input is retired and the production encoder emits the release before fresh input. `backpressure-{red,green}.log` covers actual pipe bytes. Ordinary UI send failures also request the independent release and clear local held state.
2. End-of-frame modifiers could corrupt keyboard shortcuts. Each event now uses its own modifier snapshot; the real workspace/owned RDP server observes Ctrl down, C down/up, Ctrl up. The same real UI case reproduced and fixes navigation keys stealing canvas focus and clicks leaking through covering windows. Repeated focus requests no longer erase egui's focus filter. `modifier-red.log`, `overlay-red.log`, `focus-filter-red.log`, `ui-review-green.log`.
3. Server shrink could turn queued, previously valid pointer coordinates into a fatal error. Move/scroll coordinates now clamp to the current framebuffer; stale out-of-bounds button presses are dropped, while releases remain deliverable. The real helper/owned RDP server case watches resize 800→640 before submitting old coordinates. `shrink-{red,green}.log`.

A separate real endpoint regression proves a closed TLS connection retires a pending sign-in form (`auth-disconnect-{red,green}.log`). The held-input release test now waits for peer-observed presses before release: the new barrier correctly discards presses still queued when focus is lost, so assuming those presses reached the peer was invalid.

Native Windows/macOS, Windows RDP servers, broad codec/security/provider interoperability and original Personal 26.6 comparison remain evidence gaps. The Windows log is a source-only parent composition check, not a full native build or helper runtime proof. Final whole-suite results are recorded in README.md and PARITY_PROGRESS.md after the gate.
