# P05 cleanup fairness fix

The fixed-prefix cleanup considered only the first eight retirement records. Eight live old terminals therefore kept every later closed release from being reclaimed. Cleanup now scans the entire registry snapshot; installs cap retained records at 128. Cancellation remains checked before every record and throughout owned-file hashing. Cleanup still runs outside the registry lock, and the pinned authority, identity checks and preservation of uncertain replacements are unchanged.

The regression installs and repairs actual pinned BusyBox bytes nine times, retains eight shared command leases, then retires and closes the ninth release. It verifies cleanup removes the ninth files while preserving all eight live releases. Once their leases close, a further cleanup removes all eight. The original implementation failed on “closed ninth release was starved”; the revised implementation passed. This is a real on-disk release and lease lifecycle, without synthetic retirement metadata.

Evidence:

- `cleanup-fairness-red.log`: expected assertion failure, exit 101.
- `cleanup-fairness-green.log`: five package library cases passed, including fairness, preservation and cancellation.
- `cleanup-fairness-workflows.log`: four real Linux package and PTY workflow cases passed.
- `cleanup-fairness-ui.log`: actual Tools-button startup and cancellation case passed (one test).
- `cleanup-fairness-patch-dry-run.log`: shared patch applies to the main tree after the B01 merge with zero fuzz and without mutation.
- `cleanup-fairness-source-sha256.json`: exact private source hashes and shared patch hash.

The fairness change is entirely in the new owned `src/packages/mod.rs`; shared-hook behavior is unchanged; the patch was regenerated against the current main baseline to preserve B01 additions without fuzz. The private package identity remains `0.1.0-p05-private` and must not be copied into the application manifest. No source changes were made in the main tree. Full integrated-tree and native Windows validation are outside this focused check.
