# X03 independent review

Reviewed the private handoff in `target/agent-work/x03` against the approved X03 plan and existing owned-display lifecycle. This reviewer did not implement the change. Review covered the seven-file integration patch, native implementation, subprocess supervision, UI lifecycle, tests, manifest, and recorded qualification evidence.

## Important findings

No blocking correctness or security findings in the proposed Linux slice.

- Native ABI layouts and function signatures match the exercised Linux Xlib/GLX/Composite/RandR interfaces. GL, Composite, and RandR libraries remain loaded through `XCloseDisplay`, preserving Xlib extension close callbacks. Fixed-size GL readback is 64×64×4 bytes; PNG encoding uses that actual buffer. XEvent storage has the required 24 native longs. Native allocation/error exits can leave resources until this disposable child exits; they do not accumulate in the UI process or survive its process termination.
- RandR chooses an actual advertised size, observes root geometry and a screen-change event, and requests and checks restoration before returning success. Its state-sensitive event functions are resolved before changing mode. Failed restoration makes the entire probe fail. Child crash, invalid output, timeout, or cancellation prevents Ready and stops the fresh owned server, so interrupted native work is not exposed to clients.
- Production executes the current application directly with an internal argument, without a shell or PATH lookup. Explicit executable injection is used by fixtures. Display and private authority are scoped to the child environment; native libraries are not loaded into the UI. The existing authority, localhost-only/no-TCP, engine verification, monitor rejection, and owned-process lifecycle code are unchanged. Non-Linux engines were already unavailable and remain unqualified.
- Probe supervision polls cancellation with an eight-second deadline, kills and waits its immediate child, rejects unsuccessful exit, and limits JSON reads to 65,536 bytes. The three-slot UI channel accommodates the maximum Capabilities/Ready/Ended sequence without rendering; shutdown requests cancellation and joins the worker. The fresh-server failure branch explicitly stops the lease before completion.
- Capability evidence separates extension advertisement from successful rendering and separates direct GLX from GPU qualification. Unsupported optional libraries and provider modes remain errors/unavailable capabilities. Linux software GLX, opaque Composite copy/readback, and RandR evidence do not establish alpha composition, native Windows/macOS, GPU, cross-monitor behavior, dependency closure, or original-product equivalence.

## Verification

Independently verified all seven private source hashes, all four unchanged MAIN hook hashes, and the integration patch hash against `handoff-hashes.json`. `patch --dry-run -p1` accepted all seven files against MAIN. Independently decoded the report PNG: 64×64 RGBA, exactly four expected colors with 1,024 pixels each, byte-identical to the saved `gl-scene.png`.

Inspected the final recorded graphics run (7 passed), isolated UI run (4 passed), and UI unit run (4 passed, 4 opt-in ignored). These are implementation-owner runs, not reruns by this reviewer. No build, full suite, live X operation, host display access, or production edit was performed during review. The parent's full regression gate remains separate and must be reconciled before checkpoint advancement.

## Verdict

**APPROVE the narrow Linux integration patch.** Keep X03 scoped/partial until the parent completes integration verification and reconciles the outstanding platform, hardware, and original-equivalence evidence. This verdict does not mark the whole X03 checklist outcome complete or advance the user checkpoint.
