# Optional Wayland helper test prerequisite guard

2026-11-25. Parent-authorized ROOT test-only correction. The only production-tree test source changed is `tests/packaging_wayland.py`: a class-level `unittest.skipUnless("E03_HELPER" in os.environ, ...)` now skips the two optional helper qualification tests only when the environment key is absent. No helper existence/truthiness check is used. Explicit empty or invalid inputs still execute and fail. All staging, distinct-binary, source/lock/license/hash and native ABI assertions are unchanged. No other MAIN source or frozen private handoff was edited.

Evidence:

- `optional-helper-fix-red.log`: prior absent-key discovery executes two cases and errors on KeyError.
- `optional-helper-fix-absent.log`: after the guard, ordinary absent-key discovery reports **2 explicit prerequisite skips**, exit 0.
- `optional-helper-fix-real.log`: with the already-built private integration helper supplied explicitly, **2 passed**, exit 0. No Cargo/helper build or GUI test was needed.
- `optional-helper-fix-bad.log`: explicit nonexistent helper, selected staging case: **1 case / 1 hard error, 0 skips**, expected exit 1.
- `optional-helper-fix-empty.log`: explicit empty value, selected staging case: **1 case / 1 hard error, 0 skips**, expected exit 1. These are intentional negative test runs, not passing qualification evidence.

`optional-helper-fix.patch` contains the single-line decorator addition; base/candidate SHA-256 files identify the exact test change. No native source, screen, clipboard, input, permission dialog or service was accessed. All owned subprocesses finished. Full ROOT gate and fresh scoped review remain the coordinator's responsibility; E03/progress/native qualification is unchanged.
