# B04 implementation handoff — 2026-11-24

Overall approved-plan progress remains **60.0% (36/60)**. B04 remains pending native Windows qualification; this is an implementation/review handoff, not a completed parity claim.

## Source and integration boundary

Work was performed in `target/agent-work/b04/source/`, copied privately with target/docs excluded. Its Cargo package version was changed to `0.1.0-b04-private` before any build. MAIN production sources were not edited. Review material is under `target/agent-work/b04/review/`: five new owned source/test files, `shared.patch`, `base-hashes.json`, and hashes for the new files. The shared patch intentionally excludes the private version change.

New owned files:

- `src/platform/path_map.rs`: explicit absolute Windows drive and UNC paths and `/drives/<letter>/...` mapping; Unicode and shell metacharacters remain literal. Rejects controls, traversal, ADS, device namespaces, ambiguous trailing dots/spaces, wildcards and ordinary reserved DOS device names. A final `\.` supplied by Explorer is normalized safely so quoted drive roots work.
- `src/platform/registry_fs.rs`: native Win32 bounded key/value enumeration, typed value reads and confirmed writes. Supported editors cover REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ, DWORD, QWORD, BINARY and NONE. Multi-string editing uses a lossless JSON array representation; binary editing does not truncate existing bytes. Malformed/unknown text types cannot be silently repaired by the editor. Windows access-denied errors are surfaced with their error code.
- `src/platform/windows_integration.rs`: native per-user Explorer folder/background menus, `termcraft:` URL and `.termcraft-link` registrations. Names and marker/command/value snapshots declare ownership. All locations are preflighted before install/removal; unrelated/edited registrations are refused and preserved. Inherited HKCR registrations are checked before introducing a user override. No UserChoice/default-application settings are written. Originally existing entries are never replaced, so no restoration of prior values is needed; absence is the only original state changed. Partially failed native creation is preserved rather than aggressively cleaned up.
- `src/ui/windows_platform.rs`: actual Windows-only Tools sections for status/install/remove and registry browse/read/preview/confirm. Registry browsing starts read-only. Work runs on a background thread with a single bounded result channel, cancellation and explicit errors. Values live only in ephemeral UI state; no registry values are serialized into workspaces or logged.
- `tests/windows_integration.rs`: cross-platform behavioral fixtures and native Windows-only temporary HKCU regression cases.

Narrow shared edits add the modules/Win32 registry feature, the Windows Tools section, mapping in real `--cwd`, URL folder/bookmark folder and `--open-file` flows, strict handler-only activation flags, a read-only native uninstall guard before workspace IPC, and a supplied-installer guard hook. The reviewed B05 launch parser remains the conservative version-1 contract: no credentials, commands or workspace override in URL/file content. Existing workspace persistence and theme/branding were not changed.

Windows packaging helper edits were explicitly coordinated/authorized by root for the review fix. `packaging/windows/install-per-user.ps1` now starts the installed executable with the constant `--windows-integration-uninstall-guard` argument, waits for its exit code, and refuses before any file move/delete on failure. The native guard only reads named HKCU registrations and refuses while owned/edited values refer to that executable; unrelated handlers remain. README instructions describe the enforced lifetime guard. No PowerShell/cmd registry implementation or evaluated command string was introduced.

## Verification evidence

Every Cargo invocation used `CARGO_BUILD_JOBS=2`. Component tests and final review-fix Windows checks use a private target directory. Earlier shared cache use was limited to `cargo check`, which does not create/overwrite the unversioned production executable.

- `red.log`: the planned new module API was absent before implementation.
- `typed-red.log`: typed editor API was absent before implementation.
- `root-red.log`: drive-root Explorer command quoting and executable percent-substitution checks failed before the fixes.
- `editor-red.log`: binary truncation and drive-relative `C:` regression checks failed before the fixes.
- `multi-red.log`: lossless multi-string edit API was absent before implementation.
- `status-red.log`: an existing conflict was hidden by another missing entry before the fix.
- `inherited-red.log`: inherited-handler safety API was absent before implementation.
- Initial `final-linux-tests.log`: **12 tests passed**, compiling and executing the actual private production path/registry/ownership/CLI/launch modules and actual `tests/windows_integration.rs` through a small component harness. This exercises Unicode/metacharacter paths, forbidden paths, ownership install/remove preservation, changed/prior handlers, inherited handlers, lossless typed editors, cancellation, and real CLI path routing. It does not execute native Windows APIs.
- Initial `final-windows-check.log`: actual production Windows registry, path, registration, CLI, launch and Windows Tools UI sources plus actual native regression tests passed `cargo check --offline --tests --target x86_64-pc-windows-msvc` in the component harness. Only UI-unused warnings arise because the full app is not instantiated in that harness. The harness aliases `eframe::egui` to the actual egui dependency; no registry/launch/UI implementations are replaced by stubs.
- Initial `final-app-check.log` (before review fixes): private full application `cargo check --offline --lib --bin harbor-workspace` on Linux passed; existing unrelated dead-code warnings remain.
- Initial `final-fmt.log`: private application `cargo fmt -- --check` passed (empty output).

Current scoped review-fix evidence supersedes the initial scoped runs:

- `review-fixes-red.log`: new guard and cancellation APIs were absent before the fixes; tests failed to compile at the missing behavioral boundary.
- `review-fixes-final-linux.log`: **16 actual-source tests passed**. Added tests require a valid strict activation boundary, reject separately supplied workspace options, preserve explicit trusted `--open-url` + `--data-dir`, refuse package removal for owned/edited references while allowing unrelated handlers, cancel install/remove/status during preflight without mutations, and stop additional mutations after cancellation following one completed creation.
- `review-fixes-final-windows.log`: actual Windows production modules, Tools UI and native regressions passed the focused MSVC `--tests` component check. The new native command-line test obtains the actual registered command, substitutes crafted quote-bearing URIs, uses Windows `CommandLineToArgvW`, then calls the actual CLI parser. The native package-lifetime test uses only an isolated temporary HKCU fixture to verify guard refusal for owned/edited references, preservation of edited values, and success after owned cleanup. These native tests remain unrun.
- `review-fixes-final-fmt.log`: formatting passed (empty output).
- A shared-cache component check that was waiting behind root's full gate was explicitly terminated; its log is not success evidence. The final Windows check ran independently in the private component output directory.

Commands are reproducible from the workspace root:

```text
CARGO_BUILD_JOBS=2 cargo test --offline --manifest-path app/target/agent-work/b04/component/Cargo.toml
CARGO_BUILD_JOBS=2 cargo check --offline --manifest-path app/target/agent-work/b04/component/Cargo.toml --tests --target x86_64-pc-windows-msvc
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<app>/target cargo check --offline --manifest-path app/target/agent-work/b04/source/Cargo.toml --lib --bin harbor-workspace
CARGO_BUILD_JOBS=2 cargo fmt --manifest-path app/target/agent-work/b04/source/Cargo.toml -- --check
```

## Qualification and review limits

**nativeWindowsunavailable**: this Linux host cannot run native Windows tests. Six native-only cases are authored and component-checked but **unrun**: isolated HKCU install/remove with unrelated-entry preservation, actual stale-value write rejection, actual ACL-denied browsing, CLI/URL Windows mapping, package-lifetime guard refusal/owned cleanup, and actual Windows command-line quote-injection rejection. All native test writes are restricted to unique temporary `HKCU\Software\TermCraftTestFixtures\B04-<pid>-<nonce>` subtrees; none access real association keys for writing. An earlier GNU Windows component attempt failed because that target is not installed; final checks use the installed MSVC target.

Actual Explorer menu execution, received native process working directory (including drive root and UNC), URL/file association behavior, cancellation during native operations, native PowerShell/package install/upgrade/remove and ACL behavior still require native Windows validation. PowerShell is unavailable here, so the updated actual installer workflow was not executed. The argv-routing test does not substitute for a real Explorer launch. Full application test-suite and merged-source gates are reserved for root after fresh review/integration; this worker did not produce an unversioned binary or claim the full suite passed.

Fresh-value/ownership comparisons occur immediately before native writes/removal, but query and mutation are separate Win32 calls. They are not a cross-process atomic compare-and-swap guarantee against concurrent external edits in that small interval. Cancellation cannot undo a write that already reached Windows. These are explicit native-review limitations, not passing concurrency evidence.

## Review fixes and integration handoff

The three findings in `review.md` are addressed in the refreshed private bundle:

1. Protocol/file/folder registrations now select `--windows-activation-url`, `--windows-activation-file`, or `--windows-activation-cwd`. The CLI accepts exactly two argv entries for these boundaries and never returns a data directory. Extra workspace/action flags are refused before URL/file parsing. Ordinary trusted desktop flags retain their existing semantics. The native package guard is a separate exact-one-argument early Windows entry after the existing P06/browser bootstraps and before workspace selection/IPC; it only inspects references.
2. The supplied package uninstaller enforces the native read-only guard before moving or deleting program files. It refuses owned/edited references or inspection errors and preserves unrelated data. Successful removal is available after the existing owned Tools cleanup; a manual instruction is now enforced rather than optional documentation.
3. The same cancellation token reaches status/install/remove preflight, inherited checks, native snapshot enumeration and checks before each native create/set/delete step. Cancellation stops future mutations, including rollback mutations; completed/partial owned work remains visible and the error explicitly directs a status refresh. Ordinary non-cancellation failures retain owned-only rollback attempts. The UI sends and displays the actual operation result instead of suppressing it after cancellation.

Current review bundle contains five new owned files plus eight narrow shared-file hunks with base hashes: Cargo.toml, platform/mod.rs, platform/launch.rs, cli.rs, ui/tools.rs, main.rs, Windows README and install-per-user.ps1. All eight base hashes matched MAIN at final handoff. No MAIN production edits or real registry mutations were performed; all worker commands finished before handoff. Root supplies scoped re-review and merged gates.
