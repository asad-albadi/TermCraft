# Printer FIX1: spawn-time Darwin owner identity

Status: ready for scoped 939 re-review. The reviewed P1 startup adoption race is reproduced against the original watchdog source and contained by the replacement in controlled Linux POSIX fixtures. Native macOS service/runtime validation remains unavailable. No MAIN writes, system printer/service calls, native framework access, hardware output, or whole-application gate were performed.

## Frozen base and exact scope

`base/` is an immutable copy of the previously reviewed rebased candidate, with `base-map.json` recording its 4,050 source/evidence-input file hashes. All nested target/dist/node_modules/__pycache__ directories were excluded. The original rebased packet is unchanged and all its 21 payload hashes were reverified.

`payload/fix1.patch` applies **after** the original reviewed 21-file printer packet. It changes exactly three integration-source paths:

- `engines/rdp/src/printer_native.rs`: pass `std::process::id()` as an immutable spawn argument; initialize Darwin ownership from argument 2 before `serve` can decode INIT or call any backend; retain the same private worker/protocol/limits and cleanup behavior.
- `engines/rdp/src/printer_owner.rs`: validate the supplied positive owner PID against actual `getppid` before any native access, then watch that same expected identity. The watcher checks before its first sleep, so owner death between initial validation and watchdog scheduling cannot be mistaken for trusted adoption. A mismatch exits only this helper via `_exit`; no foreign PID is signalled. Two pure tests cover explicit owner parsing and rejection of adoption.
- `tests/printer_owner_lifecycle.py`: deterministic Linux POSIX scheduling regression, using the exact old watchdog statements and the exact new owner module. Printing/backend calls are replaced by a controlled blocking callback; no native printer/framework is invoked.

`payload/source-map.json` records original reviewed-base and final hashes for the three scoped paths. `preservation.json` verifies the other **20 reviewed integration-source files** unchanged after private identity normalization, and **4,043 frozen-base files** unchanged. Card production/API/worker/component code, shared main/card dispatch, UI/options gates, queue/UUID rules, Linux Binding, PDU bounds/driver announcements, native backends and wait-error quarantine remain unchanged.

The private helper/component package names are fix1936 identities to prevent stale artifact reuse; VERSION remains 0.1.936 and the helper BIN remains termcraft-rdp-printer936. Root BIN remains termcraft-native-printer936. The four helper/component manifest/lock identity-only differences are not integration patch hunks. Two existing generated legacy-component files were regenerated only to point at this private source root; no generator or generated source changes are part of FIX1.

## Actual RED/GREEN ownership evidence

Command (from the workspace root):

```
python3 app/target/agent-work/d02-native-printer/fix1/candidate/tests/printer_owner_lifecycle.py --old-source app/target/agent-work/d02-native-printer/fix1/base/engines/rdp/src/printer_native.rs --output app/target/agent-work/d02-native-printer/fix1/evidence/owner
```

The script compiles uniquely named old/new standalone Linux fixture executables. Linux subreaper state belongs only to this test process. Exact old watchdog statements are extracted from the frozen reviewed source; the new fixture includes the unchanged production owner module. Standalone POSIX declarations are Linux fixture bindings, not a claim of Darwin runtime ABI validation. Every blocking backend callback is a sleep loop in the fixture.

1. **Actual old-source RED:** stop the child immediately before watch initialization, queue a valid framed INIT from the real owner, exit the owner, then resume the child. Original source trusts the adopted subreaper and enters the blocking backend callback. The owned orphan is then killed and reaped in finally.
2. **New-source GREEN, same forced ordering:** original-owner PID was already passed as a spawn argument. The reparented child exits 1 before the validated marker, INIT decoding or backend marker. It is reaped.
3. **Normal live-owner startup:** expected and actual owner agree, buffered INIT reaches the fixture callback, child exits 0 and is reaped by its still-live owner.
4. **Owner death after initialization:** callback is deliberately blocked after valid initialization; the same expected-owner watchdog exits the helper within the observed quarter-second polling interval, and the helper is reaped.

An unrelated owned control child survives every case and is then separately reaped. `owner-lifecycle.json` records all cases, exact old/new source hashes, unrelated-child survival and final reaping. This test is explicitly Linux POSIX lifecycle evidence; no actual Darwin CUPS/runtime behavior is claimed.

## Covering verification

All Cargo invocations were sequential, offline, locked, with two jobs and unique fix1936 package identities. `command-results.json` records exact commands.

- Helper unit target: **69 passed**, including accepted Card framed tests, all preserved printer cases, wait-error classification and the two new owner tests. No fresh PC/SC integration run is claimed.
- Dedicated exact-source printer component: **12 passed**.
- Printer component `cargo check --tests`: **x86_64-pc-windows-msvc passed**, **aarch64-apple-darwin passed**. Darwin compiles the production owner module with its real libc definitions; this is Rust typechecking, not native SDK linking or runtime execution.
- Final lifecycle script: old-source RED and all three new-source GREEN cases passed; all owned fixture children reaped, unrelated control survived before its own cleanup.
- Changed Rust formatting and Python syntax passed. Scoped patch replay matches all three payload hashes. Exact tested production/card/shared/component input hashes and log hashes are frozen in `tested-source-map.json`.

The accepted six unchanged native component checks remain in the original rebased packet; FIX1 reran the two printer checks whose source graph changed. No broader application gate or unrelated provider work was added. Already printed bytes, OS acceptance/cancellation limits, the trusted-administrator queue UUID race, and all prior native-runtime/hardware limitations remain unchanged. D02 remains open.
