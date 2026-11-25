# X02 independent review

Two important findings remain in session startup. No critical finding was identified in the reviewed changes.

1. **Resolve SSH configuration before deciding whether to start the owned display.** The UI tests the saved X11 checkbox in `src/ui.rs:963`. The worker subsequently resolves SSH configuration in `src/ssh/worker.rs`, and `src/ssh/config.rs:457` can override the checkbox when the session prefers its SSH config. A new Owned profile with the checkbox clear and preferred trusted forwarding enabled reaches the worker without a display lease and fails even when the engine is installed. In the opposite direction the UI can wait for a missing engine and reject a shell whose effective configuration disables forwarding. Exercise both directions through normal session launch. The first should open one owned display and request forwarding once. The second should connect even with a missing bundle and leave the worker idle.
2. **Surface a missing System display before an explicitly X11-enabled OpenSSH launch.** The empty-DISPLAY guard in `src/ui.rs:976` covers Local only. An OpenSSH/System session with X11 selected takes the generic process launch path. OpenSSH can open its shell with no X11 forwarding when the caller has no display. Add a launch-level regression with empty DISPLAY that expects a visible display error before spawning OpenSSH, alongside coverage for an ordinary shell with forwarding disabled.

The existing evidence records 3 isolated SSH/X11 protocol tests, 5 isolated UI tests, 5 SSH X11 unit tests and 4 SSH authentication tests. The review inspected this evidence and the code without running a live display. The forwarding path checks the per-connection cookie before connecting to the local X socket. Relay expiry observes both session and display generation. Unix process environment overrides are child-local. The isolated Local test checks the two selected child variables and the parent environment. It does not separately observe a sibling child's environment.

The evidence is Linux-specific. Native macOS and Windows engines, desktop clipboard bridging and original-runtime comparison remain unverified. The parent batch owns whole-suite verification.

## Follow-up review

1. **ADDRESSED — Effective embedded SSH options.** The worker resolves the SSH profile once, requests a display only for an effective owned terminal forwarding request, and resumes with that same profile. The UI matches the connection ID and supplies a one-shot response to the originating handle. Connection cancellation interrupts the wait, and pending IDs are filtered against live tabs. This exchange operates on runtime state and introduces no settings writes during import. The isolated UI evidence covers both config precedence directions and a config edit during startup.
2. **ADDRESSED — OpenSSH/System with empty DISPLAY.** Launch validation now checks explicit System forwarding before spawning OpenSSH. The no-forwarding case retains explicit OpenSSH flags. The isolated UI evidence covers both cases.

The follow-up evidence records 7 passing isolated workspace tests, 3 passing isolated forwarding tests, 5 passing X11 SSH unit tests, 4 passing SSH authentication tests and 11 passing SSH option tests. This re-review inspected source and recorded results without starting a live display. Both original important findings are closed.
