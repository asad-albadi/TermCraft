# P06 Tools pointer workflow

Progress: 58.3% (35/60 implementation checkpoints). Test-only delta: `p06-handoff/ui-pointer-test.patch`. The approved production portion of `src/ui/plugins.rs` is unchanged, apart from the separating blank line before the appended test module. No product bug was found and no production change is proposed.

The single Linux x86-64 test renders the actual Workspace Tools page and finds controls by painted text and clipping rectangles before sending real egui pointer press/release events. It compiles a local native fixture, creates its real archive/manifest, supplies the inspected review at the native-picker boundary, and then performs installation through the rendered **Install disabled** button. The native file-picker dialog itself is outside this test.

The workflow observes the rendered native-execution request, confirms disabled installation has not executed any code, opens the explicit full-account consent, cancels it and verifies that the grant remains absent. It then accepts the rendered grant, verifies enabling alone does not invoke code, runs the declared check through the actual worker, and observes its bounded success status and exactly one real invocation marker. A hanging second invocation hides Run controls; another production PTY responds while it is busy. Clicking **Cancel** produces the worker's cancellation error. Clicking **Disable** removes both Run controls, persists disabled state, and leaves the invocation count unchanged while the other PTY responds again.

The harness disables egui opening animations to make pointer locations deterministic. Initial harness attempts checked disclosure text before the animated collapsing region had fully painted; those were test timing/setup errors, not product findings. No lifecycle mocking or test-only production hooks were added.

Focused command (private checkout, shared dependency cache, CARGO_BUILD_JOBS=2):

```sh
cargo test --offline --bin harbor-workspace ui::plugins::pointer_workflow_tests::tools_plugin_pointer_consent_invocation_cancel_disable_keeps_live_pty_usable -- --exact --nocapture
```

Observed: **1 passed, 0 failed, 139 filtered out**, 1.56 seconds execution. Evidence: `p06-ui-pointer.log`. `rustfmt --edition 2024 --check src/ui/plugins.rs` also passed (`p06-ui-pointer-fmt.log`). This command built only the hashed binary test executable; it did not build or replace the shared unversioned app.

Test patch SHA-256: `b2a5150187ba6554f481650efca4e5c70b4dec082b5f94b3557ccaf0e4269fbb`.
Approved shared implementation patch remains: `fd0b00dd64432566cd9bfe55a6ffc25acf74fcc83fa2691531f2be9e9f7a6f55`.
The test patch dry-runs cleanly against merged MAIN `src/ui/plugins.rs`.
