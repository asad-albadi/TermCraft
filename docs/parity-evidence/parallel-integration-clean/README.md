# Fresh main workspace verification

After cleaning only the root package artifacts, `CARGO_BUILD_JOBS=2 cargo test --all-targets --offline` exited 0: **565 passed, 0 failed, 20 ignored, 58 outer targets**. Nested fixture subprocess summaries are excluded from the totals. The separate locked RDP helper run passed **41 unit and 8 provider tests**. Four queue tests passed. Captured root source hashes remained unchanged during the run.

The private P05, P06 and B01 candidates are outside this source cut. Their package identities now differ from the main package so the shared Cargo cache cannot substitute their libraries/test executables. The root package-only clean forced a fresh main library and target rebuild. Native Windows/macOS execution is not established by this gate.

The 20 opt-in tests remain recorded as ignored in this unfiltered gate. Separate checks against the frozen combined release archive passed all five isolated X11 cases and the real empty-PATH SSH/Mosh UDP workflow. Both Wayland and X11 production startup checks and all 980 packaged file hashes also passed. See `../P01/combined-release-verification.json`. These runs qualify the tested Arch-compatible Linux snapshot, with its declared system dependencies.
