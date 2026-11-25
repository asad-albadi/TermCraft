# D02 printer FIX1 scoped re-review 939

Verdict: **ACCEPT**. The sole prior P1, Darwin startup adoption before owner capture, is resolved. This is a scoped FIX1 decision following the completed original full review; it does not close D02 or claim native runtime/hardware validation.

The parent appends its immutable process identity when spawning the worker (`printer_native.rs`). Darwin parses that explicit argument and calls `owner::watch` before `serve` can read INIT or call any backend. `printer_owner.rs` compares actual parent with the expected owner immediately, then its watchdog repeatedly checks the same captured expected identity, starting before its first sleep. A startup adoption therefore fails initialization; a death between initial validation and watcher scheduling remains detectable. Mismatches exit only the helper; no unrelated PID is signalled. Windows JobObject behavior and the reviewed wait-error quarantine remain intact.

## Independent evidence

- Applied FIX1 once after the previously reviewed packet in the existing isolated review939 candidate. All three final hashes match the frozen payload, and the only changed private source paths are `printer_native.rs`, `printer_owner.rs`, and `printer_owner_lifecycle.py`. All other previously reviewed private source files remain byte-identical, including manifests, card modules, UI/options, queue backends, Linux provider and protocol limits. Frozen FIX1 tested-source hashes independently verified. See `audit.json`.
- Ran the exact supplied lifecycle script independently against the original reviewed watchdog source and exact new production owner module. Old-source RED enters the blocking backend after adoption. New-source GREEN exits 1 before validation/INIT/backend; normal live-owner startup exits 0; death after validated initialization exits 1 in 0.253 seconds. An unrelated owned child survives every case and is then separately reaped. These are controlled Linux POSIX process fixtures, not macOS CUPS/runtime tests. See `owner/owner-lifecycle.json`.
- Focused helper native tests: **10 passed, 0 failed**, including two owner tests and preserved native framing, permission, cancellation/reaping and wait-evidence cases. The other 59 cases were filtered; no repeat original full suite is claimed.
- Exact-source printer component `cargo check --tests`: Windows x86_64-pc-windows-msvc and Darwin aarch64-apple-darwin both passed. Every Cargo call was sequential, offline, locked, `-j 2` and `CARGO_BUILD_JOBS=2`, retaining existing private review939 identities. See `command-results.json` and logs.

Reproduction from app root:

```
python3 target/agent-work/d02-native-printer-review/candidate/tests/printer_owner_lifecycle.py --old-source target/agent-work/d02-native-printer/rebase/payload/files/engines/rdp/src/printer_native.rs --output target/agent-work/d02-native-printer-review/fix1/owner
```

Cargo commands ran from `target/agent-work/d02-native-printer-review/candidate`; exact argument arrays are frozen in `command-results.json`.

All owned lifecycle fixtures and control children were reaped; no fixture executables remain active. All commands finished. No MAIN changes, broader root tests, native service/system printer access, SDK linking or hardware output.
