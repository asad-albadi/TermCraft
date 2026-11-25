# B05 scoped fix review — 2026-11-24

Verdict: **approved for integration; original Important finding resolved**. No new Important defect found in the transaction/locking changes. This verdict covers exception recovery and serialization, not native macOS validation or power-loss durability.

Reviewed the two-file delta `target/agent-work/b05/b05-review-fix.patch`, SHA-256 `0d7639833a2ca9e1210a3c19226ea49a94b75bfd1f061cb4844d425bc47c1446`, and final owned source in the refreshed full patch, SHA-256 `bccbe668e24c7dbab7443d11602c301e65eae371703e5864c2e5b3208126cde5`. Both checksums were verified.

- `packaging/linux/desktop-integration.py:124-132` checks ancestor directory types before installed-file mutation, addressing the real first-install directory conflict.
- Lines 137-161 stage the receipt and replacement bytes, preserve original files by rename, track new/obsolete files, and publish ownership only after file operations succeed. Lines 162-185 restore completed changes in reverse order on failure. The original receipt survives a failed publication; newly introduced files are removed and obsolete originals restored. Rollback retains user replacements and preserves backups with an explicit location if recovery itself fails.
- Lines 78-104 and 193-195 serialize install, status and remove through the same persistent lock inode. The descriptor rejects symlinks/nonregular files, closes on all paths and is not inherited across executable launches. Keeping the inode after removal avoids split-lock races.
- The focused regressions exercise both original failure reproductions, actual execution of the restored old launcher, direct retry/removal, new/obsolete helper recovery, and a real second installer waiting until the first rolls back. Inspected RED logs confirm the original failures; inspected GREEN logs show focused transaction tests **3/3**, full Python desktop tests **8/8**, and focused package tests **2/2** passing. No unresolved uncertainty warranted repeating these tests, and no Cargo/application build was run during this review.

The prior native macOS limitation remains unchanged. Parent integration and whole-tree verification remain separate gates; preserve current main-tree P06 source and README changes when applying the reviewed patch.
