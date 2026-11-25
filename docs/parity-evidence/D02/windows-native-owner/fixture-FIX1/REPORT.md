# Windows serial owner fixture packaging FIX1

Status: DONE. Scoped packaging correction complete; awaiting parent's scoped re-review and mechanical integration. No MAIN writes or commits were made. All production source is unchanged, including `serial_windows.rs` SHA-256 **9b13c1a1615f23c49e585d8004bf9111f58336d9d6f46812f0e9ba50e73be489**.

The original fixture used `tests/windows-serial-owner-component/main.rs`. Cargo automatically discovers that path as an application integration-test target, so root formatting tried to load the fixture's ungenerated support files. The corrected fixture entry point is `tests/windows-serial-owner-component/src/lib.rs`; its bytes are identical to the old main.rs. The component manifest points there, the generator writes its support files beside it in src/, and the README describes the location. No generated support source is required for root target discovery or formatting.

## Scope and files

The integration delta removes one file, adds one file and modifies three files:

- Remove `tests/windows-serial-owner-component/main.rs`.
- Add `tests/windows-serial-owner-component/src/lib.rs` with exactly the removed file's bytes.
- Modify that component's `Cargo.toml`, `generate.py` and `README.md`.

The component's Cargo.lock is unchanged and included in the hash/preimage map. The final payload contains only the five final fixture files. `preimages.json` includes the null preimage for the added file; `hash-map.json` includes the null final value for the removed file. Production source is explicitly pinned separately in manifest.json. The private source identity is 0.1.9879 with app/helper names termcraft-win-owner-fix1-9879 / termcraft-rdp-win-owner-fix1-9879. Identity changes exist only in private build inputs and are excluded from integration.

## RED evidence

A fresh private root-view copied the parent's integrated MAIN Cargo manifest/lock, src, tests and vendor inputs. It contained the five original fixture files and no generated fixture support source.

`CARGO_BUILD_JOBS=2 cargo +1.95.0 metadata ... --offline --locked --no-deps --format-version 1` returned **72 root targets**, including an unintended test named windows-serial-owner-component with src_path ending in tests/windows-serial-owner-component/main.rs (`red-metadata.json`).

`CARGO_BUILD_JOBS=2 cargo +1.95.0 fmt ... --all --check` failed because completion.rs was absent beside that main.rs (`red-fmt.log`). This reproduces the integration finding before changing the fixture layout.

## GREEN evidence

Exact commands appear in COMMANDS.md. Only the scoped standalone fixture was compiled; no app/helper or broad test build was repeated.

- Fresh root metadata on both installed Cargo 1.95 and stable 1.99 returned **71 targets**. The sole removed target is the unintended owner fixture; every other target name, kind and crate type is identical to the red inventory (`metadata-check.log`, `rename-proof.log`, green-metadata*.json).
- Root `cargo fmt --all --check` passed on both 1.95 and stable 1.99 in the private root-view, which still has no generated completion.rs/helpers.rs/serial_windows.rs/serial_stats_windows.rs/inputs.json in its fixture src directory (`green-fmt.log`, `green-fmt-stable.log`). No source generation ran in MAIN or root-view.
- The relocated component was separately regenerated in private source and its full `cargo +1.95.0 test --offline --locked -j 2 -- --test-threads=1` passed **27 tests, 0 failures**, plus 0 doc tests (`owner-component.log`). Every Cargo invocation set CARGO_BUILD_JOBS=2.
- `rename-proof.log` proves no fixture body byte changed during relocation. `product-unchanged.log` proves the Windows product file's exact unchanged hash. No repeat Windows typecheck was needed for a fixture-only move; the prior production typecheck remains the relevant evidence.
- `fix1.patch` reconstructs the exact add/remove/modify/unchanged map against the six-path baseline (`reconstruction.log`). Observed MAIN preimages match that baseline. The original 693-entry owner evidence seal remains valid (`prior-seal-check.log`). This candidate has its own evidence.sha256 seal; old reports/seals are untouched.

## Integration packet

Use `integration-packet.md`, `fix1.patch`, `preimages.json`, `hash-map.json` and `payload/`. Apply only this fixture delta on top of the integrated owner candidate. Delete the removed main.rs as the patch specifies. Generated outputs belong only in the separately generated component fixture and are excluded from the integration payload.

This corrects local test-target packaging only. The previously stated Windows hardware/runtime limitations remain unchanged and are documented in prior-REPORT.md. No additional design ruling or production behavior change was made.
