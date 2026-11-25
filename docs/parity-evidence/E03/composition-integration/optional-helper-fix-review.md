# Optional Wayland helper test prerequisite review

2026-11-25. **Approved; no Critical or Important findings.** Scope is solely the presence-only prerequisite decorator in `tests/packaging_wayland.py`. This does not reopen the accepted VNC composition or qualify any native platform.

Independently verified all entries of `optional-helper-fix-evidence-sha256.json`, the actual candidate SHA-256 `3620bb04d52f52d82d1ce730d0aeeac1a9b9234d7af811650158802b713895cc`, and the recorded base against the frozen accepted integrated test file. Removing the single added decorator reproduces the accepted base exactly. All test bodies and staging/hash/source/lock/notices/ABI/distinct-binary assertions are preserved. The patch accurately describes the actual change.

The class guard tests `"E03_HELPER" in os.environ`, not truthiness or file existence. Absent optional qualification input yields explicit skips instead of KeyError during ordinary discovery. An explicitly provided bad or empty value still selects the test and reaches normal packaging validation; it cannot be hidden as an unavailable prerequisite.

Independently reran only these focused Python cases:

- Absent environment key: unittest discovery, 2 explicit prerequisite skips, exit 0.
- Explicit existing private helper `target/debug/e03-vnc-integration-wayland-private`: discovery, 2 passes, exit 0.
- Explicit nonexistent helper: selected staging test, 1 hard error, zero skips, exit 1 as expected.
- Explicit empty value: selected staging test, 1 hard error, zero skips, exit 1 as expected.

No Cargo build, broader composition review, native capture/input/clipboard, permission dialog or service operation was performed. Packaging used temporary stub app/RDP binaries and the existing explicit private helper; all subprocesses completed and temporary packaging directories were removed. ROOT production behavior is unaffected by this test-only change. E03 remains open; progress stays **42/60 (70.0%)**, strict native/original qualification **0**.
