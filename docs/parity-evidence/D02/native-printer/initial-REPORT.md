# Rebased native-printer source for review 939

This frozen private candidate is based on the MAIN snapshot taken after accepted Smartcard935/review938 and the current Mosh/game/registry changes. No MAIN writes, native spooler calls, hardware output, or whole-application gate were performed. The original native-printer packet remains immutable and its payload hashes were reverified.

## Exact snapshots and scope

- `base/` and `base-map.json`: immutable 4,050-file captured baseline. The count includes 3,420 historical parity-evidence files, not compiled component targets. No nested `target`, `dist` or `node_modules` was copied.
- Twelve historical `__pycache__` files were recorded in `excluded-build-only.json` and removed only from candidate. `curated-base-map.json` selects 812 source/test/direct-document inputs, excluding historical evidence outputs and all build-only directories.
- `candidate/`: complete private build source. Root VERSION 0.1.936 / BIN termcraft-native-printer936; helper VERSION 0.1.936 / BIN termcraft-rdp-printer936. The helper PRIVATE package name is termcraft-rdp-printer-rebased936 and the printer component PRIVATE name is termcraft-printer-native-rebased936, preventing reuse of original candidate/RED artifacts. Product integration payload restores the baseline product package identities and omits the private root manifest/version changes.
- `payload/source-map.json`: exact base/candidate/integration hashes for 21 owned source files. `core.patch`, `options-ui.patch` and `harness.patch` are the patches to apply once, against this captured base. `payload/files/` contains their exact final integration source.
- `preservation.json`: verifies all 4,025 unowned, non-build-only baseline files are unchanged in candidate, and records preserved printer-family hashes plus feature unions. Original baseline/payload packet remains under the parent native-printer directory.

## Four shared files mechanically rebased

1. `engines/rdp/Cargo.toml`: retain all accepted smartcard Windows features; union only Printing/Gdi (JobObjects was already present), add Tokio `process`, extend tempfile to supported native platforms. No accepted feature removed.
2. `engines/rdp/src/main.rs`: add native --printer-worker dispatch before the accepted cross-platform --pcsc-worker dispatch. The accepted PC/SC dispatch/body is preserved.
3. `src/desktop/options.rs`: apply printer capability/queue-validation hunks while preserving the accepted cross-platform smartcard capability. Persisted option fields/defaults are unchanged.
4. `src/ui/rdp_settings.rs`: apply the explicit printer mapping gate/label/copy while preserving the accepted smartcard reader gate.

All accepted smartcard provider/ABI/transport/API source files and their native component files are byte-identical to this base. Unowned game, Mosh, registry and other product source is included only as baseline/build input and is hash-preserved. `shared-files.json` records old/current/payload hash identities for these four overlaps.

## Printer-only lifetime correction after the original packet

Self-review found that the original cleanup treated `Ok(Err(wait_error))` as successful reaping. The rebased actor now classifies `Reaped`, `Pending`, and `Unknown`; only nested `Ok(Ok(exit_status))` proves reaping. A normal nonzero exit also proves reaping. A timeout permits process termination only after the owned Child's `try_wait` returns live; an ambiguous wait error never triggers a possibly reused Unix PID. A final unknown/error/timeout retains the Child, global permit, and Windows JobObject ownership handle without invoking kill-on-drop. This deliberately consumes one of sixteen permits until owner exit rather than creating unlimited uncertain processes/resources. The same bounded cleanup covers Windows ownership-assignment failure before any queue initialization.

`original-packet-safety.patch` compares only this new actor/test/design correction to the original printer packet; it is already included in `core.patch`, not an additional patch to apply. The shared frontend/PDU code, Linux provider body, native Windows/Darwin backends, retained socket IPP codec, and Windows ownership wrapper remain byte-identical to the original packet; their hashes are in preservation.json.

## Existing device component generation

The initial extra legacy device check was attempted without its required generated input and reported absent drives.rs. This was a setup omission, not a MAIN defect. The existing generate.py was then run in private candidate. Expanded printer cfg would pull the unrelated printer completion/SVC contract into this deliberately extracted Drives/serial/native-file component. The narrow `harness.patch` therefore excludes only the production printer module declaration from that extraction. It adds no fake or stub APIs. The dedicated printer component separately compiles the exact complete production printer sources. The existing generated drives.rs and production-input-sha256.json are generated evidence, not source payload; regenerate them after applying the generator hunk.

## Final sequential verification

All Cargo calls used two jobs, offline, locked, with no overlapping private Cargo invocations. Final logs in `payload/evidence/`:

- Helper: **67 passed**, no failures; includes all twelve preserved Linux IPP fixtures and ten controlled native/retained-stream cases.
- Printer component: **10 passed**, no failures.
- Exact-source `cargo check --tests`: printer, accepted smartcard, and generated legacy device component, each for **x86_64-pc-windows-msvc** and **aarch64-apple-darwin**: all six passed.
- New wait-evidence RED control uses unique red-wait936 identity and deliberately classifies wait error as Reaped; the regression fails Reaped versus expected Unknown. Final GREEN classifies normal exit, wait error and timeout correctly. Existing controlled child test also cancels and confirms reaping of a real intentionally hung owned process. The 67 helper cases include both accepted smartcard framed-transport tests. The separate PC/SC integration target was not rerun in this rebase; prior accepted PC/SC evidence is not claimed as a new run.
- Changed Rust formatting passed; all 21 integration payload hashes match a replay of the three patches against base. Exact tested production/helper, accepted smartcard, component inputs and log hashes are recorded in tested-source-map.json.
- No owned protocol fixture processes remain. All worker tool sessions/Cargo calls are complete.

Native SDK linking, Windows/macOS actual spooler runtime, native driver error/abort behavior, remote Windows driver negotiation, Easy Print/XPS, and physical output remain unverified. Darwin's IPP check/submission cannot impose an atomic UUID precondition against privileged local-administrator queue replacement; that trusted-administrator boundary remains documented. OS acceptance and already printed bytes cannot be undone; jobs are never automatically retried or cancelled using stale IDs. D02 remains open.
