# Windows serial native owner component

This Linux fixture compiles the production Windows Serial, Handle, Pending, statistics probe, control validation and completion encoder. The generator preserves source bodies and replaces only the Win32 namespace, Windows OS-handle adapters and owner thread-spawn boundary. It records original input SHA-256 hashes. The shared-dispatch loop is controlled test scaffolding; its file-read branch uses the local filesystem. This is not Windows hardware execution.

From the app root:

```sh
python3 tests/windows-serial-owner-component/generate.py
CARGO_BUILD_JOBS=2 cargo +1.95.0 test --manifest-path tests/windows-serial-owner-component/Cargo.toml --offline --locked -j 2 -- --test-threads=1
```

Run serially because the controlled kernel state is process-global. Tests cover stuck cancellation with other-port/file progress, actual write success and read abort, shared-binding quarantine on disconnect, sixteen retained owners/process capacity, saturation with SET_WAIT_MASK escape and CLOSE, initialization stalled inside the native owner, spawn failure before initialization/I/O, FIFO CREATE publication and unpublished/duplicate IDs, optional-probe timeout quarantine, output transformation and per-open statistics origin, and issuing-thread identity through cancellation/retirement/event drop. Every rejected queued request retains its completion ID. The fake boundary flags restoration/event destruction before native terminal completion.

The fixture entry point is src/lib.rs, so Cargo does not discover it as an application integration test. The generated src/serial_windows.rs, src/serial_stats_windows.rs, src/helpers.rs, src/completion.rs and src/inputs.json are fixture outputs. Generate before building. The real Windows native component separately checks production Win32 types and signatures with an MSVC target.
