# Local original-package inspection and quarantine import

This implements a bounded subset of P05/P06/E04: inspect a user-selected original ZIP container, show its decoded inventory, and explicitly import the reviewed files into a separate owned quarantine. It does not establish full original package-manager or native-plugin compatibility. No parity, native or strict/full completion count advances are justified.

`src/packages/legacy.rs` accepts an ordinary single-disk ZIP without requiring a custom TermCraft manifest. Every ZIP member is read through EOF for CRC validation. Ordinary members remain ordinary; recognized LZMA-alone payloads are independently decoded with bounded dictionaries, contexts and output, with no fallback to raw data on decoder failure. Encoding is determined from bytes, never from filename or an inferred version. There is no universal LZMA magic; conservative header recognition may refuse ambiguous binary members rather than silently importing them as decoded content. ZIP64, self-extracting and multi-disk archives are outside this implementation.

The generated `local-import.json` uses `termcraft.local-legacy-import.v1`. It is explicitly **local metadata**, not a recovered original vendor schema. It records the source archive SHA-256, source filename, decoded member hashes and sizes, member encoding, header-identified format/architecture, provenance and disabled execution status. Header identification is not loader qualification or publisher authentication. No package version is inferred. Source preview bytes are held privately, and import copies that snapshot rather than reopening a changed source archive. Every copied file is rehashed before commit.

The Tools section is "Original package archives". Inspection and import run in cancellable background workers; the job is joined on disposal. The user can review the inventory, discard it, or explicitly import files with commands disabled. No source archive is executed. Import does not modify PATH, register commands or plugins, invoke a system runtime/Wine, or change the existing pinned BusyBox/native-plugin lifecycle and grants. No original executable, DLL, package, extracted document or other original asset is included in this patch.

## Recorded reference artifacts

| Explicit prerequisite | Source | SHA-256 | Files | Decoded bytes |
| --- | --- | --- | ---: | ---: |
| `LEGACY_SQLITE_ARCHIVE` | [Official Sqlite3.mxt3](https://mobaxterm.mobatek.net/plugins/Sqlite3.mxt3), previously downloaded in reviewed original-runtime evidence | `3bd4979a8b7241b8a27b7020bb2eb08ab950ad15b2dde854c840da465f311c77` | 7 | 957458 |
| `LEGACY_CYGUTILS_ARCHIVE` | User-supplied CygUtils.plugin, reviewed original-runtime companion | `65c17f095ecbbab4c8015e24aa02bc7de1c098704aec3d8b4b7ec81b28a56602` | 974 | 71234178 |
| `LEGACY_CYGUTILS64_ARCHIVE` | User-supplied CygUtils64.plugin, reviewed original-runtime companion | `4750dc32ba3372fa7f4b8ca38583f98d9d0477d9b0686966fee40aa181bff915` | 356 | 47100773 |

Sqlite3.mxt3 is an ordinary ZIP containing four x86 PE files, two text documents, and a gzip manpage. Its members are not nested LZMA. CygUtils.plugin has 973 LZMA-alone members plus one ordinary text member; CygUtils64.plugin has 355 LZMA-alone members plus one ordinary text member. All actual files were inspected, explicitly quarantine-imported and rehashed; temporary imports were automatically removed. The 64-bit companion includes 74 x86_64 and 3 x86 PE headers, so even that container name is not a safe per-file architecture assertion. Complete generated inventories are retained as the three `LEGACY_*_ARCHIVE.json` files; these contain metadata, not original member bytes.

Seven decoded game SHA-256s match the independent, already reviewed Python/LZMA results in `../original-static-recovery/controls.json`. Sqlite's decoded sqlite3.exe SHA-256 `2c49f68dc4775cffb5e9db6e9f34214abbde3cdae03d2fb6380da66d7b4592a4` was independently confirmed using Python's ZIP reader. No executable was launched. The Cygwin README remains an imported document; no unproven dependency resolver or package-version semantics are derived from it.

The raw embedded MOBASE resource and its CRC-anomalous recovered ZIP are not accepted evidence for execution. This importer rejects raw non-ZIP resources and rejects invalid ZIP CRCs.

## Containment and limits

- Source archives must be regular files opened without following the final symlink and without blocking on FIFOs.
- The 128 MiB source limit and 4096-entry / 4 MiB central-directory limits apply before ZIP metadata allocation. Strict ZIP header consistency rejects local/central mismatches, hidden entries and overlaps; preflight rejects duplicate names before a ZIP reader can deduplicate them.
- ZIP output is independently capped at 16 MiB per member and 128 MiB total. Nested LZMA decoded output has separate 16 MiB per-file and 128 MiB aggregate limits. Dictionary size is capped at 16 MiB and lc+lp at 8; recognized oversized, corrupt, truncated or trailing-data streams fail.
- Paths reject traversal, absolute paths, backslashes, NTFS streams, Windows devices, trailing dots/spaces, case conflicts, duplicate entries, file/directory conflicts, links and special files. Metadata may preserve ordinary archive directory structure only.
- The owned quarantine root is opened without following a replacement root symlink. New Unix roots use mode 0700; payloads receive no executable permission. Capability-relative staging, a regular nonblocking exclusive import lock, digest revalidation and a unique staged directory prevent partial installation and replacement of existing imports. Failed/cancelled stages are removed; only a completed directory is renamed into place.
- Cancellation is checked while reading source/ZIP/LZMA bytes, between files, while copying imports, and before commit. Cancellation after the single commit point is not a rollback request.

## Validation and reproduction

The private work copy used unique package version 0.1.142 and bin `legacy-package-compat-private`, with `CARGO_BUILD_JOBS=2`. The handoff restores the project's original version and implicit bin. No broad GUI or full application suite was run.

Recorded focused validation:

- Initial RED: both generated manifest-free ordinary ZIP and nested LZMA fixtures rejected by the unsupported stub.
- Additional RED: a ZIP with different local and central filenames was accepted before strict header validation; it is rejected after the fix.
- Generated legacy/security tests: 16 passed, 1 explicit original-archive test ignored by default.
- Full isolated package subsystem: 18 passed, 4 ignored (the original-archive gate plus three existing BusyBox retirement prerequisite tests).
- Explicit three-reference-archive gate: 1 passed; all decoded files copied and hashed, seven independent game hashes matched, execution disabled.
- Narrow Tools worker/result tests: 3 passed, including a real generated ZIP reviewed and imported off the caller thread.
- Isolated package/codec library checks for Windows MSVC and Apple aarch64: compile checks only, not native execution qualification.

Normal project commands after integration:

```sh
CARGO_BUILD_JOBS=2 cargo test --lib packages::legacy
CARGO_BUILD_JOBS=2 cargo test --bin harbor-workspace legacy_package_
```

The actual-reference test deliberately fails for missing environment prerequisites when explicitly selected. It never silently succeeds without the artifacts:

```sh
CARGO_BUILD_JOBS=2 \
LEGACY_SQLITE_ARCHIVE=/path/to/Sqlite3.mxt3 \
LEGACY_CYGUTILS_ARCHIVE=/path/to/CygUtils.plugin \
LEGACY_CYGUTILS64_ARCHIVE=/path/to/CygUtils64.plugin \
cargo test --lib legacy_actual_reference_archives_import_without_execution -- --ignored --nocapture
```

Optional `LEGACY_IMPORT_REPORT_DIR` writes generated metadata reports during that test. Logs and private base/proposed-file hashes are retained in the handoff.

## Codecs and remaining gaps

The [zip maintainers' security policy](https://github.com/zip-rs/zip2/blob/v9.8.2/SECURITY.md) supports the latest release and explains that strict header consistency is opt-in in 9.x. The implementation pins zip 9.8.2, enables only flate2 compression support, and explicitly validates full header consistency. Nested LZMA uses the independent [lzma-rs 0.3.0 implementation](https://github.com/gendx/lzma-rs). Registry source checksums, exact licenses and upstream repositories for every newly locked dependency are retained in `codec-provenance.json` and `licenses/`. No system package installation or machine configuration was performed.

Original MobApt dependency resolution, original command discovery/registration, a supported Windows/Cygwin execution provider, dependency availability, native grants for original code, lifecycle integration with runnable original packages, native Windows execution qualification, and universal original package semantics remain open. This importer exposes no executable launch route on any platform. Existing P05/P06 packages, grants, launch leases, cancellation, file identity and retirement behavior remain independent and unchanged.
