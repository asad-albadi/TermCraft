# D02 partial implementation evidence — 2026-11-24

D02 is **not complete**. Counts remain 25/60 implementation checkpoints, 0/60 fully verified parity outcomes. The latest handoff is at the end of `../../../PARITY_PROGRESS.md`.

Latest resume: [listener investigation, commands, results and remaining work](resume-validation.md). The current post-review failure inventory is [resume-post-review-failures.md](resume-post-review-failures.md); the older files below remain historical evidence.

Fullscreen connection propagation and desktop-vault controls now have a local implementation increment; their real endpoint/UI test is blocked, so neither is claimed fully verified.

Local passing evidence:

- `options-offline.log`: 7 options/profile/default/security/lifecycle tests pass; the live endpoint case is explicitly filtered only in this targeted run.
- `ui.log`: real egui pointer clicks change clipboard and keyboard settings.
- `engine.log`: 2 separate helper tests pass. Empty local content produces the real IronRDP initialization messages before readiness; Unicode clipboard responses obey direction, size and format validation.
- `full-final.log`: includes passing transport policy and clipboard-state lifecycle regressions, plus every current full-suite failure. No endpoint tests were hidden or newly ignored.
- `build.log`, `helper-build.log`, `fixture-build.log`: app, independently built headless helper and owned fixture all build offline.
- `rebuild-check.txt`, `startup.png`: build/startup rendering pass; original runtime mechanics/appearance comparisons remain unverified.

Observed regressions fixed in this increment:

1. Missing actual Windows profile launcher; the new lifecycle test initially failed because its API was absent. It now reads the actual owner-only UTF-16 file, verifies username/profile bytes and checks deletion after the last clone drops.
2. Unsupported mstsc view-only/custom CA/layout choices were ignored; `installed_adapters_reject_options_they_cannot_enforce` failed at “mstsc cannot enforce view-only” and now passes with explicit rejection.
3. `text_channel_enforces_both_directions_and_rejects_invalid_data` failed at “invalid clipboard data was delivered”. The generic library decoder tolerated an odd-length response. The backend now requires aligned, terminated, valid UTF-16 and rejects oversized text.

Environment limitations:

The unfiltered full suite exits 101: raw summed result lines are 256 passed / 126 failed / 6 ignored (including duplicate nested-child counts; not a new top-level checkpoint count). Fifteen targets fail. `full-failures.md` names each failure and retains its diagnostic. Loopback and Unix listener creation is denied with EPERM; several fixture wrappers consequently report readiness/agent/child-process assertions instead of the original listener error. These are not passing endpoint tests.

The independent Craft Anything build-command runner refused the test command because approval is required under a never-approval policy. `rebuild_check` was available and passed build/startup rendering, with original comparisons still incomplete.

Pending implementation: embedded device/admin providers. Desktop vault/fullscreen have a newer increment with endpoint/UI validation pending; full clipboard UI/endpoint interoperability is also pending. Native Windows/macOS, hardware and original Personal 26.6 runtime comparison stay pending. Unsupported-provider errors do not count as implemented redirection. D03 has not started.
