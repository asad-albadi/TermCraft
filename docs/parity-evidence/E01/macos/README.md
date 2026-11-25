# macOS Browser source candidate

The shared private Unix pipe/process and Chromium rendering worker now compile for macOS instead of returning platform-unavailable before startup. Discovery checks the Chromium and Google Chrome payloads inside `/Applications` and the user's `Applications` directory. A bounded Mach-O eligibility check accepts native Intel/Apple Silicon executable slices, including canonical universal containers, and rejects wrong CPU, library images, contradictory slices, oversized tables and out-of-file ranges. macOS retains responsibility for executable loading and signing policy.

The worker preserves the separate private profile, inherited private descriptors, standard Chromium sandbox/TLS checks and owned process-group teardown. Device scale is forced to one for consistent rendered-frame and pointer coordinates. The Linux history/resize fix remains shared protocol code.

Verification:

- `red.log`: the eligibility tests failed because the new validator did not exist.
- `green.log` and `production-tests.log`: three architecture/container-boundary tests passed against the actual production module.
- `component-check.log`: `cargo check --offline --target aarch64-apple-darwin` passed for the production Browser modules and actual System-discovery module. The small checking crate supplies only the enclosing catalog interface and provider enum; it does not replace the pipe, worker, parser or discovery implementations. Its dependency lock starts from the application's lockfile. This verifies component types and Darwin API availability, not the entire application or native execution.
- Final real Chromium Linux viewport/egui/auth/cleanup tests are recorded in `../review.md`. No browser process ran on a Mac.

Native Intel/Apple Silicon execution, signing/Gatekeeper, packaging, Retina and platform clipboard/IME behavior still need native qualification. Windows transport remains separate implementation work. No E01 checkpoint advances from this source-only candidate.

Format references consulted 2026-11-24:
[Apple Mach-O header](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/EXTERNAL_HEADERS/mach-o/loader.h) and [Apple universal container header](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/EXTERNAL_HEADERS/mach-o/fat.h). The parser is independently written; it checks eligibility and bounds rather than implementing a loader.
