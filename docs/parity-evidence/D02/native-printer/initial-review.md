# D02 native printer independent review 939

Verdict: **CHANGES REQUIRED** — one concrete ownership finding.

## P1: Bind Darwin watchdog to the original owner before allowing native access

`engines/rdp/src/printer_native.rs:403–409` captures `getppid()` only when the newly spawned helper begins executing, and thereafter exits only when that captured parent changes. The parent actor can already have buffered INIT (run's initial exchange) while the child has not yet reached this capture. If the RDP owner exits during that scheduling interval, the helper starts under its adopted parent (normally launchd), records that as its owner, consumes buffered INIT, and enters `Native::open` / its initial readiness request. An indefinitely blocked native call then outlives the true owner indefinitely: EOF does not interrupt the synchronous backend call, the true owner's cleanup has disappeared, and the watchdog sees no further parent change. The global sixteen-permit counter also disappears with the true owner, so repeated owner restarts do not contain these orphans. This contradicts the required owner-death containment of potentially blocking native methods.

Reproduce without printers or native service access:

```
python3 target/agent-work/d02-native-printer-review/owner-race.py
```

The script extracts the exact production watchdog statements into a uniquely named standalone Rust fixture. It pauses immediately before those statements (a deterministic scheduling delay), makes a launcher queue a valid INIT then exit, resumes the helper, and substitutes an intentional blocking operation for native open. Linux's subreaper supplies the adopted parent so the reviewer can reliably kill and reap the owned fixture afterward; it is not claimed as an actual macOS service test. `owner-race.json` records that the captured initial parent equals the review subreaper and the helper survives the owner's exit. The identical source-level adoption gap applies to the Darwin watchdog. The fixture is killed and reaped in `finally`.

Pass the known owner's identity at spawn and reject mismatched/reparented ownership before any backend initialization, or establish equivalent owner-death observation before permitting INIT. Keep the watchdog for death after that validation. Add a controlled owner-exits-before-watch-initialization case; the existing hung-child test only exercises parent-initiated cancellation while the owner stays alive.

## Reconstruction and preservation

- All three authoritative patches were applied once to a fresh private captured-base copy. The explanatory original-packet-safety patch was not reapplied. All 21 reconstructed files exactly match integration payload hashes.
- All 4,050 captured-base hashes, 53 tested source-input hashes, and 4,025 unowned preservation hashes verified. Each existing owned current-MAIN source matches the rebased baseline hash. Original packet payload hashes also verified without edits.
- The six original printer-family files remain byte-identical from original packet to rebased packet. Linux Binding differs from accepted baseline only in visibility required by extraction; common PDU code differs only in user-facing spooler notice text. The original twelve IPP fixtures are preserved.
- Shared Cargo/main/options/UI hunks retain accepted smartcard feature union, dispatch and capability/gates. All dedicated smartcard source/ABI/worker/component inputs are preserved. Native printer modules are omitted only from the existing extracted Drives/serial harness; the dedicated printer component compiles complete production printer sources.
- Reviewed retained HPRINTER RAW flow, active-document-only abort, fixed root-protected Darwin socket/peer checks, UUID readiness/presubmit checks with explicit privileged-administrator race boundary, no reconnect/auth fallback, private anonymous spool files and bounds, one-slot commands, poison/no reopen, close-stdin-before-kill cleanup, and wait-error quarantine. Only nested successful wait proves reaping; unknown waits retain Child/permit/Windows ownership resources.

## Fresh validation

Private source root: `candidate/`. Version 0.1.939; root/helper names termcraft-printer-review939 and termcraft-rdp-printer-review939; private component and fixture names are unique. Every Cargo call uses offline, locked, two jobs. No concurrent private Cargo calls or overwrites of running fixture executables. Copied source mtimes were refreshed before compilation.

- Helper unit target: **67 passed, 0 failed**, including preserved real-socket IPP cases, accepted smartcard framed tests, native protocol tests, real pipe permission mapping, cancellation/reaping and wait-evidence classifications. `helper-unit.log`.
- Printer component: **10 passed, 0 failed**. `printer-component.log`.
- All **six native Windows/Darwin component checks passed**: printer, accepted smartcard and generated legacy device, both x86_64-pc-windows-msvc and aarch64-apple-darwin. See `component-results.json` and individual logs.
- Initial broad helper `cargo test` also tried the separate pre-existing PC/SC integration target, which refers to the product executable name. That target cannot compile after private renaming without fixture adaptation; the reviewed 67-case unit target was then selected explicitly. This is private review setup, not a product defect. No fresh PC/SC integration run is claimed.

No MAIN edits, actual hardware output, native spooler/service access, native SDK linking or whole-application gate. The startup-race fixture was killed and reaped. All review commands completed before final handoff; all owned fixture jobs are reaped.
