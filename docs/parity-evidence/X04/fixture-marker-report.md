# Private XDMCP fixture marker publication repair

## Scope and diagnosis

Only `target/qualification-games-v03/xdmcp-fixture-atomic/xdmcp_window.py` differs from the copied immutable V03 fixture. The four other fixture files are byte-identical. Every source fixture file was checked against `docs/parity-evidence/V03/run-qualified/source-input-sha256.json` and matches. No production source, frozen executable, V03 evidence, or earlier successful history was modified. No Cargo invocation or broad suite was run.

The observed failure is recorded in `docs/parity-evidence/parallel-integration-games-v03/extracted-xdmcp.log`: the second-session geometry assertion reads an empty string, expecting `1280x720`. The consumer (`src/x11/xdmcp_engine.rs:664–686`) waits for marker existence, then reads once. The producer uses `Path.write_text`, which opens/creates/truncates the marker before writing its payload. A reader can therefore see existence while the target is empty.

The focused probe deterministically reproduces this condition using the original fixture's actual publication statement. It holds the producer at the first real stream write, after the target has been created, and observes `target_exists=True; target_bytes=b''`. The atomicity assertion fails for exactly that reason. This establishes a real synchronization defect independent of X11 rendering and monitor selection.

Alternative causes considered: wrong geometry would normally produce a nonempty payload with incorrect dimensions; wrong-cookie/display-open or green-pixel failure raises before publication and leaves no marker. Neither directly explains the empty published target observed in the recorded failure. The fixed probe uses the same payload expression. These observations do not claim the full XDMCP integration now passes; root's fresh actual integration rerun remains required to exclude an additional issue.

## Minimal change

The private producer now calls a small `publish_marker` function. It creates an exclusively owned random temporary file with `mkstemp` in the marker's existing private test directory, writes the full unchanged payload, closes the stream, then atomically replaces the marker using `os.replace` on the same filesystem. A `finally` block removes the owned temporary after success or failure. No marker is exposed before close/rename. The real X11 display-open authorization, window mapping, green pixel check, geometry queries, and 20-second client lifetime remain unchanged. The original Rust assertions and frozen test binary remain unchanged.

The marker's parent comes from the existing Rust case's private `tempfile` directory; the temporary is mode 0600 and created by exclusive `mkstemp`, rather than sharing a fixed temporary name.

The exact diff is `fixture-marker-patch.diff`. Base/candidate SHA-256 values, baseline manifest identity, frozen binary/consumer hashes, and evidence hashes are in `fixture-marker-hashes.json`.

## Deterministic focused verification

`fixture-marker-test.py` parses the selected fixture, compiles its real import/function nodes and its actual publication call, and supplies only the X11 query inputs and invocation environment. It does not implement a second publisher. A wrapper around real `io.open` stream writes uses two threading events to hold publication after file creation and before any payload is written. File creation, writes, close, replace, and cleanup are real filesystem operations. No sleeps select the observation window.

Commands, actual exit statuses, and logs are captured in `fixture-marker-test-results.json`:

- Original characterization: exit 0; target visible with empty bytes while writing is held.
- RED, atomicity expectation against immutable original: exit 1; `AssertionError: publication exposed incomplete marker: b''`.
- GREEN, same expectation against private candidate: exit 0; target absent while writing is held, then exact full payload after completion, with no temporary remaining.
- Injected real write-boundary failure against candidate: exit 0; expected `OSError`, no target and no temporary remaining.

The RED check was first run against the copied unmodified candidate before implementation; the final recorded RED rerun selects the identical immutable source so the result stays reproducible after the fix.

Each producer thread is joined in a `finally` path before an assertion can fail. Every probe's private temporary directory is removed on exit. All subprocess probe invocations completed and were reaped. No X11 servers, display managers, Cargo jobs, or fixture clients were started by this work.

## Root review and remaining validation

Use `target/qualification-games-v03/xdmcp-fixture-atomic` as the fixture directory for the existing frozen `target/qualification-games-v03/lib-tests` exact actual XDMCP case. Review the minimal diff and hashes first. The actual extracted-package integration rerun is deliberately left to root as assigned; this report records focused publication verification only.
