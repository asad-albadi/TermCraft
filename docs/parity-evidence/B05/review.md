# B05 independent implementation review — 2026-11-24

Disposition: **changes requested — one Important finding**. Reviewed the private final owned source and the 15-file patch `target/agent-work/b05/b05.patch`, SHA-256 `067aedbce01fb22dc2d962cb767522eb34b4d1b810c26879c8384b3617315ba4`, against approved B05 and the global/shared contracts. No production source edits or binary builds were made during review.

## Important finding

**[P2] Restore the previous owned installation when installation fails.** `packaging/linux/desktop-integration.py:107-110` replaces installed files and removes obsolete files before atomically publishing the new ownership receipt, without rollback or a recovery journal. Staging the input bytes does not make those mutations transactional. A routine filesystem failure can therefore leave an installation which its own status, retry and uninstall operations refuse to manage.

Two isolated temporary-directory reproductions used the production Python class, with cache refresh disabled only in the injected-failure case:

- Real filesystem conflict, no injected failure: create an otherwise valid minimal package and a regular file at `XDG_DATA_HOME/applications`. Install copies the payload and launcher, then raises `FileExistsError` when creating the applications directory. `owned.json` is absent. Remove the conflicting file and retry: install now raises `Existing file is not owned by TermCraft` for the payload left by the first attempt.
- Upgrade commit failure: install an old payload, change its application bytes, then make only the final `os.replace(..., owned.json)` raise `OSError`. The new application bytes remain installed with the old receipt. Each of status, install retry and uninstall raises `Owned integration file was changed` for the application. A failure earlier in the replacement loop can additionally leave mixed release/helper files.

Preserve backups and roll back all completed mutations if the file/receipt commit fails, or use a durable recoverable transaction. Preflight directory types as well. Add a focused failure/retry regression proving the previous installation remains usable and removable, and a failed first install does not strand unowned files. Do not repair this by relaxing refusal to overwrite or delete genuinely user-modified files.

## Reviewed behavior and limits

- URL/file actions retain literal values, reject terminal commands/credentials/workspace overrides, and preserve an explicitly supplied CLI workspace. Unix bookmark opening uses nonblocking/no-follow flags plus metadata from the opened handle and a bounded read. No additional Important defect found in this boundary.
- Linux desktop quoting uses desktop-entry escaping plus literal Python `execv` arguments. Stable installed payload/icon locations preserve relocation behavior. Existing ownership hash checks retain modified files; defaults are not edited. These successful-path properties do not resolve the failed-install finding above. Concurrent installer mutation remains untested; path checks alone do not serialize an installation transaction.
- macOS source wires AppKit URL/file events and Finder Services to argument arrays and resolves the Rust executable beside the launcher, retaining Rust/helper adjacency. This is source review only: native compilation, actual event delivery, Services, registration/removal, relocation and user-default preservation remain unavailable on this Linux host. No native success is claimed.
- Packaging adds the launcher and registration script under the existing package root, checks launcher architecture/libraries, and includes them in the existing package manifest/source snapshot. The explicit asset allowlist remains unchanged. The patch does not edit `main.rs`, `lib.rs` or UI source; integration must preserve the newer main-tree P06 hooks and apply only the narrow `platform/mod.rs` module additions.
- Existing reported evidence (Rust desktop 4, Linux/source Python 5, focused packaging 2, shortcuts 5) was inspected and not redundantly rerun. The two failure-path probes above address a previously untested uncertainty. Whole-tree checks after integration remain the parent's gate; native macOS/original-runtime evidence remains pending.
