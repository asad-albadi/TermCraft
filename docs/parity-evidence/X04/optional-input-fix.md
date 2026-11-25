# X04 optional archive input discovery correction

2026-11-25. Scope: only `tests/packaging_xdmcp.py`. Root's combined full run had failed because standard discovery implied an absent `source-packages/xephyr.pkg.tar.zst` input.

The actual pinned-payload test now uses `unittest.skipUnless('TERMCRAFT_TEST_XEPHYR_ARCHIVE' in os.environ, ...)` and reads the supplied value with `os.environ[...]`. An absent key explicitly skips this optional-input contract. A present key, including an invalid or empty value, executes the existing staging checks and fails when invalid. The real pin, notices, dependencies, and corrupted-archive refusal assertions are preserved.

Validation:

- `env -u TERMCRAFT_TEST_XEPHYR_ARCHIVE PYTHONDONTWRITEBYTECODE=1 python -m unittest discover -s tests -p 'packaging_*.py'`: 27 tests, OK, 4 skips (this optional archive test and three existing prerequisite cases). `optional-input-fix-absent.log`.
- Explicit real `target/agent-work/x04/source-packages/xephyr.pkg.tar.zst`, discovering `packaging_xdmcp.py`: 1 passed, including corruption refusal. `optional-input-fix-real.log`.
- Explicit `/nonexistent/termcraft-missing-xephyr.pkg.tar.zst`: 1 failed with the expected `ValueError: Expected a regular pinned Arch Xephyr archive.`; zero skips. `optional-input-fix-invalid.log`.

Changed test SHA-256: `311b3a1dacfe05b156c72c098edd7315c81cf5cc92fcd90cc08ec32583d52c5f`. This correction adds no native/runtime qualification and closes no parity item; overall remains 40/60 (66.7%).
