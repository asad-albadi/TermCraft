# Plugin fixture recovery (private candidate)

The failure is a test fixture protocol race. The compiled C fixture writes its reply and returns without reading stdin. Therefore it can close its input before the Rust parent reaches its unconditional `write_all(b"{}\n")`. The parent fullgate recorded errno 32/BrokenPipe at that exact write (`src/plugins/host/unix.rs:193`). This diagnosis does not identify a production PluginChild defect; a previous gate passing the same fixture is consistent with scheduling dependence.

## Scope and hashes

- MAIN base `src/plugins/host/unix.rs`: `3026341c6f1cbee61b0135953d425b54523a40f43730c16b568dd40ee5374faf`.
- Proposed file: `308099082909608838fd9d2a7fe4ac8bdd1eb9a7cdacc337b57e7dd373876a95`.
- Exact copies: `unix.rs.base`, `unix.rs.proposed`; normalized single-file patch: `candidate.patch`.
- Private checkout: `app/target/agent-work/plugin-fixture-recovery/app` relative to original workspace root. Package version `0.1.913`; allocated binary name `termcraft-plugin-fixture913`; both crate roots touched before shared-target compilation.
- MAIN source has not been edited. The only proposed change adds a 256-byte `fgets` before the fixture response and rejects EOF. This consumes the parent request before child exit. No write errors are ignored, and no sleeps/retries are added to the Rust regression. Existing sealed-image mutation/EPERM checks, installed-path replacement, and original-true output assertion are unchanged.

## Deterministic protocol RED/GREEN

`probe_fixture.py` extracts the exact embedded C template from its Rust input and compiles it using `cc -Wall -Wextra -Werror`.

- RED (`red.log`, expected status 1): the unmodified producer sends unsolicited JSON; reading stdout to EOF and waiting for process completion establish that it exited before any input. Writing the request then raises BrokenPipeError. No scheduling sleep establishes this result.
- GREEN (`green.log`, status 0): the fixed producer has no response or exit during a bounded readiness observation; it accepts the request, emits the expected original JSON, and exits successfully.
- Reproduction: `python3 docs/parity-evidence/plugin-fixture-recovery/probe_fixture.py docs/parity-evidence/plugin-fixture-recovery/unix.rs.base` (RED), then the same command with `unix.rs.proposed` (GREEN), from MAIN app.
- The 250ms selector observation exists only in the evidence probe, not the proposed Rust test. Child processes are killed if necessary and waited in finally cleanup. BrokenPipe handling only records the expected RED observation and cleans up its already-failed buffered write; it does not change the proposed regression.

## Focused Rust validation

Cargo command (private working directory; every Cargo invocation uses two build jobs):

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR='<MAIN app>/target' cargo test --offline --lib plugins::host::unix::verified_image_regressions::mutation_after_verification_executes_original_elf -- --exact --nocapture
```

Initial focused test: 1 passed, 0 failed, 258 filtered (`focused.log`). The emitted hashed `--lib` test executable `target/debug/deps/harbor_workspace-26b0083c104d9a9e` was then called directly with the same exact test filter 40 times: 40/40 passed (`focused-repeat.log`). These repeats demonstrate stability under these runs, not exhaustive scheduling coverage; protocol synchronization is the reason for the fix.

## Counterexample and restored validation

`unix.rs.counterexample` and `counterexample.patch` contain a temporary private test-only mutation: pass `File::open(&executable)` after installed-path replacement to the unchanged production spawn, instead of passing the sealed verified image. All earlier steps, including sealed-image EPERM mutation assertion, remain in place. The focused test exits 101 and fails specifically at the existing original-true JSON output assertion (`counterexample.log`), proving it still rejects the false replacement rather than accepting it through the protocol fix. The counterexample is not part of the proposed patch.

The candidate source has been restored byte-for-byte to `unix.rs.proposed`; restored focused validation passed (1 passed, 0 failed; `restored.log`). The complete plugin test filter also passed (4 passed, 0 failed; `plugins-focused.log`). All spawned build/test/probe processes have completed and been reaped.

## Remaining integration

Independent review and application to MAIN are owned by the parent agent. The parent owns a fresh fullgate after accepted merge; this private task intentionally does not repeat the broad suite.
