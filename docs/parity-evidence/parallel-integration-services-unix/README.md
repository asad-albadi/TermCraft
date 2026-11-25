# Managed services and portable Unix integration

`CARGO_BUILD_JOBS=2 cargo test --offline --locked --all-targets --no-fail-fast` exited 0: **585 passed, 0 failed, 28 ignored across 60 outer targets**. Root source hashes captured before compilation remained unchanged. Nested fixture subprocess summaries are excluded from totals. This gate covers merged reviewed B01 managed FTP/TFTP/Telnet and P05 portable Unix source; the private P06/B03/B05 candidates are outside this source cut.

`../B01/merged-protocols.log` separately records 17 successful actual protocol cases. P05 opt-in actual BusyBox package lifecycle and GUI worker tests use the already-built main hashed binaries and retain their separate logs under `../P05/`; the 28 ignores here are not counted as passing tests. Native Windows/macOS and original-app comparisons remain open. The prior helper source is unchanged and its 49-pass gate is in `../parallel-integration-clean/`.
