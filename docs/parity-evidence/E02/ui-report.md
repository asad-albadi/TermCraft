# E02 macro editor, recording and playback

The Macros menu starts and finishes a timed recording on a connected terminal or embedded desktop. The editor can reorder and remove recorded events or add text, desktop scan codes, normalized pointer/button states, delays, terminal output waits, session focus waits and calls. Playback provides a cancel action tied to the named runtime session. The session editor can attach a startup macro. The saved-session duplication path carries the attachment forward.

An attached macro waits for its runtime connection to be ready. Named CLI invocation uses the same player and takes precedence over the attachment for that launch. Macro input routes to the runtime ID allocated at connection creation. Reconnect allocates a fresh ID and cancels work for the outgoing connection. Terminal text containing line breaks goes through the normal paste confirmation and pacing path with a single fixed target. A declined paste also cancels the remainder of the macro. Compatibility terminals with unobservable sign-in, external desktop clients and view-only desktops are outside the selectable recording and playback targets.

The desktop adapter records accepted canvas input. Authentication and certificate forms use their own controls, and they cannot start a recording. Playback sends normalized pointer positions at the current framebuffer size. Scheduled release events follow earlier queued input on the desktop transport. Cancellation uses the existing priority release path to invalidate queued input and release held keys and mouse buttons. A small staged input queue retries transport backpressure while preserving pointer/button ordering. Terminal output waits consume new decoded bytes from the bound PTY or SSH stream.

Macro JSON keeps the existing text fields and adds an optional versioned script. A macro import reads at most 4 MiB and validates script versions, limits, names and the call graph before committing to the library. Workspace backup import also bounds file reads and validates the resulting macro library. Import itself does not start execution. Session storage adds an optional startup macro name.

## Evidence

- `core-green.log` covers eight deterministic recorder/player tests, including 0/100/250 ms timing, waits, bounded catch-up, target-generation/authentication checks, cancellation and release. `core-final.log` reruns those eight and adds eight passing model tests, including old workspace and text macro round trips.
- `ui-macro.log` covers nine targeted UI and integration tests. The tests observe accepted terminal input recorded from egui into a real PTY, replay through that PTY across an active-tab change with broadcast enabled, cancellation on declined paste/reconnect, named calls gated on PTY output, startup after saved-session duplication and named CLI invocation. They also cover older macro JSON, bounded imports, and blocked unauthenticated terminal input. The VNC test drives an egui canvas, inspects recorded key and pointer events, and observes both cancellation and ordered completion releases on a real loopback RFB connection.
- `ui-rdp-final.log` covers the loopback RDP peer and bundled helper. The test rejects the start of recording while trust or credential forms are present and observes an ordered macro key press and release on the peer.

The scoped format check passed in `ui-format.log`. The batch suite ran against an earlier integration snapshot. The final implementation is covered by the scoped tests above pending the next shared batch run.

Exact MobaXterm timing units, focus and pointer behavior remain unmeasured. The test fixtures cover the TermCraft PTY, RDP and VNC transports. The native Windows and original-product comparisons remain pending. Playback operates inside owned connections and implements the evidenced startup and named invocation triggers. Calendar-based macro scheduling remains outside the established scope.


## Independent review follow-up

- A focus wait reads focus captured by the terminal widget in its owning viewport. The focused detached-terminal GUI case alternates native focus between child and root while the root displays Macros and observes the resulting input on the detached terminal’s PTY.
- Adjacent output-pattern waits consume through each match and retain the remainder of the decoded text within the existing buffer limit. The engine test covers both patterns in one chunk and a Unicode pattern split across chunks.
- A queued startup player can collect bound, authenticated output before the first playback tick. The loopback SSH regression waits for the peer to enqueue connection and initial prompt events, confirms both appear in one app poll and observes the macro reply.
- The player resets its output boundary after the terminal transport accepts an input submission. A confirmed paste establishes that boundary after its final chunk is accepted. Those boundaries prevent already-buffered output from advancing a later wait and preserve output read after acceptance. Terminal writers run asynchronously, so the boundary is local transport acceptance and does not assert peer receipt.

`review-core-red.log`, `review-ui-red.log` and `review-focus-red.log` capture the baseline regressions. The focus-only baseline rerun excludes an initial graphics-fixture cleanup error. `review-core-green.log` and `review-ui-green.log` cover the nine core and thirteen macro-filtered GUI/transport cases after the fixes. `review-rdp-green.log` reruns the real RDP macro transport check, and `review-format.log` records the scoped formatting check.

The shared macro/X11 batch later exposed a timing assumption in the SSH fixture's fixed 500 ms sleep. The fixture now explicitly releases the real peer's prompt on the Connected event and applies both real events in one production poll batch. The corrected regression and all thirteen macro-filtered tests passed. See [SSH startup regression fixture](ssh-startup-fixture-report.md).
