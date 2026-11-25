# B05 desktop integration implementation — 2026-11-24

Implementation is ready for independent review/integration; B05 native outcome is pending. Production source was edited only in `target/agent-work/b05`, using Cargo package version `0.1.0-b05-private` before any build. No Git repository, commit or merge claim. Main-tree writes by this implementer are this evidence directory only. Parent owns final integration and whole-suite checks.

## Delivered behavior

- Linux `DesktopIntegration::{install_user,remove_user,status}` plus packaged Python CLI manage hashed owned files, a stable private installed payload, icon, application menu entry, directory Open With/open-here action, termcraft scheme and bookmark MIME declaration. No shell interpolation; a Python launcher uses `os.execv` with literal arguments. Whole packaged layout, helper adjacency, native runtime and package manifests are copied intact. Registering another extracted release upgrades the stable payload. Deleting/moving the extracted release does not break installed launch/icon paths.
- Installation refuses pre-existing unowned files and symlinks. Removal refuses modified owned files, leaves unknown files/directories, and never edits `mimeapps.list` or user defaults. Cache updates explicitly target only the selected XDG user data root. No actual user/system integration/defaults were changed during testing.
- Safe desktop URL/file actions map onto existing S04 `CliAction` variants and existing workspace dispatcher. No main.rs or IPC changes were required; P06 browser/plugin helper entrypoints remain untouched. Missing session IDs retain the existing explicit `Saved session ID was not found in this workspace.` error.
- macOS declarations are backed by a separate native AppKit launcher implementing `application:openURLs:` and a Finder Services `openTerminalHere:userData:error:` provider. NSTask forwards argument arrays to the adjacent unchanged Rust executable/helper. The launcher preserves `--data-dir` for subsequent events when explicitly launched with a workspace. Packaging compiles the launcher for the application architecture and checks its Mach-O architecture and system library references. Source/layout checks are not native execution evidence.
- macOS optional packaged registration tool owns a receipt of registered TermCraft bundle paths, supports registration after relocation/update, queries LaunchServices status, and unregisters only recorded paths after refusing unrelated replacements. It never sets a default handler. Registration requires Python 3 and native LaunchServices; normal application events use the native launcher.

## Shared external launch contract v1 (also reserved for B04)

`--open-url <value>` and `--open-file <path>` may be combined only with `--data-dir`; received data cannot select a different workspace, run a command/macro, select exit behavior, or inject additional options.

Supported URL spelling is exactly:

- `termcraft://session/<percent-encoded-stable-ID>` → `OpenSession`, no cwd, no auto-exit.
- `termcraft://cwd?path=<percent-encoded-absolute-directory>` → `OpenFolder`, no auto-exit.

The parser rejects unknown targets/queries, credentials/authorities, fragments, duplicate parameters, malformed escapes, invalid UTF-8, controls, overlong input and relative cwd. It decodes once and retains literal metacharacters. It does not normalize arbitrary URLs or accept network connection credentials.

A `.termcraft-link` regular file is UTF-8 JSON, <=16 KiB, strict unknown-field rejection, version exactly 1, exactly one of `{ "version":1,"session":"stable-ID" }` or `{ "version":1,"cwd":"/absolute/path" }`. Unix opening uses O_NONBLOCK/O_NOFOLLOW, rejects nonregular files and cannot hang waiting for a FIFO writer. External desktop launchers may give local file URLs or absolute paths; only generated launcher code maps these to `--open-file` or directory `--cwd` with literal path values.

Ruling: introduced this narrow contract instead of accepting arbitrary SSH/command URLs — S04 provides saved-ID/cwd actions, while inbound credentials or arbitrary terminal commands would add an unnecessary executable input surface. Cost if later product evidence requires connection URLs: add a separately specified validated connection action, retaining v1 safe semantics.

Ruling: Linux installs an owned stable payload instead of leaving entries pointing at extracted package paths — this directly preserves launcher/helper/icon resolution after package relocation/deletion. Cost: one additional user-owned copy of the release, replaced on explicit upgrade.

## RED and GREEN evidence

Commands were run from the private copy. Every Cargo invocation set `CARGO_BUILD_JOBS=2` and used the shared main target cache with a distinct private package version. No private binary was needed by the new tests. Parent was informed that Cargo can replace the unversioned target/debug application; parent rebuilds it before CLI runtime gates.

- Initial Rust lib regression: `cargo test --offline --lib cli::desktop_tests -- --nocapture` → RED, 2 failed on unknown `--open-url` / `--open-file`, 1 rejection case passed (`red-rust.log`). The same tests then passed 3/3 (`green-rust.log`); temporary lib test module was removed before handoff in favor of integration tests.
- Initial Linux owned integration tests → RED, production `DesktopIntegration` API absent (`red-python.log`).
- FIFO startup regression: `cargo test --offline --test desktop_integration received_link_fifo -- --nocapture` → RED, actual FIFO open failed the 500ms bound (`red-fifo.log`).
- Final `cargo test --offline --test desktop_integration -- --nocapture` → **4 passed, 0 failed** (`integration-tests.log`), including FIFO rejection, literal encoded session/cwd, explicit workspace preservation, command/credential/workspace rejection, strict and oversized bookmark rejection.
- Earlier targeted `--test shortcuts` → **5 passed, 0 failed**. These are existing shortcut preference tests; new native launch evidence comes from the external GIO fixture below.
- `python3 scripts/test-desktop-integration.py` → **5 passed** (`green-python.log`). The real external `gio launch` process invokes the installed production launcher after the original package has been deleted. Captured argv prove exact termcraft URL, file and folder paths with spaces, percent signs and shell metacharacters. Tests also prove in-place payload upgrade, stable icon existence, unchanged pre-existing default contents, unowned-file survival, modified-owned-entry refusal, macOS event/declaration/source wiring, and native registration unavailability on Linux.
- `python3 tests/packaging_release.py PortablePackage.test_archive_relocates_with_empty_path_and_keeps_workspace_data NativePackageLayout.test_windows_zip_and_macos_bundle_keep_helper_beside_app` → **2 passed** (`package-tests.log`). Foreign-file refusal diagnostics are expected negative cases. macOS launcher bytes in this layout test are explicitly a Mach-O fixture; no native build/event claim.
- Changed Rust files `rustfmt --edition 2024 --check` passed. Changed Python files `python3 -m py_compile` passed. Rust integration compilation retained 3 pre-existing unused UI-method warnings from the private snapshot.

The parent explicitly requested targeted checks only; the implementer did not rerun the whole suite. The parent must run the integrated gate after review and merging.

## Native limitations and references

Linux evidence exercises the actual GIO desktop-entry parser/launcher against isolated XDG roots. It does not prove every file manager/desktop menu implementation. Native macOS compilation, Finder URL/file dispatch, Finder Services visibility/invocation, registration/unregistration, app update/relocation and user default preservation remain **unavailable on this Linux host**. No screenshot, declaration, mock launcher or cross-target check counts as native success. Original Personal 26.6 exact integration equivalence still needs its existing original/runtime validation.

Primary references consulted:

- Winit 0.30 macOS custom delegate documentation explains that URL delivery needs an application delegate: https://docs.rs/winit/0.30.13/winit/platform/macos/index.html (local installed winit source also inspected).
- Apple `application:openURLs:` documents URL and document declarations and actual event handling: https://developer.apple.com/documentation/appkit/nsapplicationdelegate/application(_:open:)?language=objc
- Apple NSServices declaration: https://developer.apple.com/documentation/bundleresources/information-property-list/nsservices
- Apple services provider runtime registration: https://developer.apple.com/documentation/appkit/nsapplication/servicesprovider
- Apple Services properties: https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/SysServices/Articles/properties.html

## Review handoff

Private root: `/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/b05`.

Patch: `b05.patch`, current SHA-256 `bccbe668e24c7dbab7443d11602c301e65eae371703e5864c2e5b3208126cde5` (supersedes the patch examined in the initial independent review).

`base-hashes.json` records original shared-file hashes; `changed-files.json` lists 15 shipping source/test/doc files. Cargo.toml/Cargo.lock private version changes, temporary test modules, logs and build artifacts are excluded from the shipping patch. Parent applies only reviewed hunks, preserving current main source and other owners' hooks. All implementer command sessions have ended.


## Important review finding fixed — owned Linux installation transaction

The independent review's one P2 was reproduced without relaxing any ownership checks:

- A real regular file at the temporary XDG `applications` directory left the first attempted install's payload orphaned. Exact test RED: `test_first_install_directory_conflict_does_not_strand_payload_and_can_retry`.
- An injected failure only at final `owned.json` replacement left the new executable with the old receipt. Exact test RED: `test_upgrade_receipt_failure_restores_old_release_then_can_retry_and_remove`.

Both failures are in `red-install-transaction.log`. Installation now preflights every new/obsolete target's ancestor directory types before publishing any payload. It stages the complete receipt before mutations, keeps original files (including their original permissions) in private transaction backups, tracks each published path, and rolls back all completed mutations in reverse order if payload/removal/receipt publication fails. New transaction files are removed only when their bytes still match this transaction; user replacements remain protected. Old obsolete helpers are restored on rollback. The prior receipt stays untouched until successful final commit. If the filesystem also refuses rollback, backups are retained with an explicit recovery location instead of being deleted. This is exception rollback, not a claim of power-loss durability.

The parent requested interprocess serialization as part of transaction correctness. A real two-process test first failed because the second installer immediately inspected a mixed first transaction. It now passes: install, remove and status serialize using `flock` on a persistent per-user lock inode. The lock is not unlinked on removal, avoiding split-inode locking races. Its contents are never modified; symlink/nonregular lock paths are rejected. The second process waits while the first transaction's receipt publication is paused, then the first rolls back and the second upgrades to its own valid release. See `red-install-lock.log` and `green-install-transaction.log`.

Focused verification after the fix:

- `python3 scripts/test-desktop-integration.py InstallFailureTests` → **3 passed**. Upgrade failure is tested with both recovery choices: retry directly from the restored old installation, and remove that old installation before reinstalling. Actual execution of the restored launcher reports the old version; retry reports the new version. The old receipt and every old file match byte-for-byte, new helper files disappear after rollback, obsolete old helpers return, and user notes/defaults survive.
- `python3 scripts/test-desktop-integration.py` → **8 passed** (`green-python.log`): the original 3 Linux cases, 2 macOS source/unavailable checks, and 3 focused transaction/concurrency regressions.
- The same two focused release package invariant cases → **2 passed** (`package-tests.log`), including actual Linux GIO launch.
- Changed Python files passed `python3 -m py_compile`. No Rust source changed in this fix; per parent instruction, the previously passing 4 Rust integration tests were not redundantly rebuilt while the parent's main-tree gate ran. No broad suites were run by this implementer.

The refreshed full patch is against current MAIN shipping baselines. The latest parent README changes were retained and the B05 section appended; no P06 main.rs/lib.rs/UI/bootstrap source is included. `base-hashes.json` now contains only existing shipping-file baselines. A scoped two-file delta against the previously reviewed implementation is `target/agent-work/b05/b05-review-fix.patch`, SHA-256 `0d7639833a2ca9e1210a3c19226ea49a94b75bfd1f061cb4844d425bc47c1446`; its two prior file hashes are recorded in `review-base-hashes.json`. Native macOS limitations above remain unchanged. All implementer command sessions ended before handoff.
