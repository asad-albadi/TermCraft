# D02 Windows statistics FIX1 scoped review

SPEC: ACCEPT

QUALITY: ACCEPT

Scope: only the three-file FIX1 delta against the exact reviewed original candidate. The original eight-file architecture and its previously accepted specification were not re-reviewed. MAIN was not edited. No subagents or broad builds were used.

## Prior finding verdict

**Exclusive native-output slice constructed before terminal native wait: RESOLVED.**

`serial_completion.rs::cancel_and_collect` now takes `wait_terminal: impl FnOnce() -> Result<u32, u32>`. It requests cancellation once, invokes that zero-argument wait, and only then coerces the output Vec into a mutable slice inside `Result::and_then`. Thus failure skips both the exclusive output slice conversion and the statistics converter. Holding `&mut Vec<u8>` refers to the Vec metadata; it does not construct the problematic exclusive slice of its separate native output allocation.

`serial_windows.rs::Pending::cancel_and_retire` captures the original request's `code` and copy of `origin` before invoking the helper. The wait closure captures only native handle/operation, executes `GetOverlappedResult(..., 1)`, and returns the terminal native byte count or native error. It does not borrow or inspect the output allocation. Only the following success converter calls `stats::completed_count(origin.as_ref(), code, output, transferred)`. The handle, operation, event and buffers remain owned by the same live Pending throughout the wait. `finished` is set after collection, as before.

Native abort/other errors retain their status and bypass normalization. Completion winning cancellation still yields its actual transferred data/count. Existing read/write encoding, purge count handling, event mask output, close response ordering, and captured old-origin behavior are preserved by the limited control-flow change. No load-bearing new issue was found in this delta.

## Independent verification

Fresh checks run from the app root on 2026-11-25:

- `verify_packet.py`: exit 0. Both cumulative and delta patches reconstruct all eight exact payload hashes; all eight relevant MAIN baseline hashes remain unchanged.
- `sha256sum --check --quiet evidence.sha256` in the FIX1 packet: exit 0, no mismatches.
- Independent Python hash linkage check: all 622 private source inputs match `source-inputs.sha256.json`; all eight source/payload hashes match the packet map; all eight reviewed-base files match the original candidate's payload; exactly the declared three files differ between reviewed candidate and FIX1.
- `order_fixture.py .../source`: exit 0. Independently inspected its assertions and production native boundary; the zero-argument wait precedes success-only slice transformation and uses captured origin. This is a source-order proof, not Windows execution or a dynamic undefined-behavior detector.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/target/agent-work/d02-windows-stats-fix1/pure-build" cargo +1.95.0 test --manifest-path target/agent-work/d02-windows-stats-fix1/source/tests/windows-serial-control-component/Cargo.toml --offline --locked -j 2`: exit 0, **22 passed, 0 failed**, 0 doc tests. The component imports the production completion, statistics and control files directly. The raw-pointer terminal-write fixture verifies six-counter conversion after simulated native completion; ordering, failed-native-wait, old-origin/reopen, purge, event masks, abort, read and write cases pass.

Inspected the sealed prior evidence for MSVC production extraction (successful 8.40s check with tests) and full Linux helper (76 unit plus 10 worker tests). These broad checks were deliberately not repeated. The red source fixture rejects the previous API; the red Rust fixture establishes API incompatibility, not an executed undefined-behavior reproduction, as the author correctly states.

## Remaining limits

No Windows device execution, Miri run, or original vendor-client equivalence test was performed. Existing unbounded ordinary data/control cancellation and Pending drop waiting remain outside this three-file correction, unchanged. These inherited limitations do not invalidate this scoped fix or constitute new findings.
