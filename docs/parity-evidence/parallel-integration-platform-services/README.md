# Platform/service integration boundary

Root `CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets --no-fail-fast` exited 0: **623 passed, 0 failed, 33 ignored across 64 outer targets**. Captured Rust, tests, scripts, packaging and helper source hashes remained unchanged. Nested fixture subprocess summaries are excluded. The root main executable was forcibly rebuilt after private builders paused.

The gate covers reviewed B03, B05, X05, managed service credential undo clearing and optional P08 Unix payload support. Four opt-in X05 live cases subsequently passed using this exact main test binary within private mount/network/PID/tmp namespaces and nested Cage; the helper-only ignored case is not a product test. Separate merged checks passed B05 Python8, Iperf packaging3 and Unix packaging11. Native Windows/macOS, actual full desktop environments and original-app evidence remain open. B04/Cron/NFS changes are outside this source cut.
