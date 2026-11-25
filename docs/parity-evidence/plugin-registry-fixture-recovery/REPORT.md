# Registry fixture recovery — private candidate

The registry regression C producers emitted JSON and exited without consuming the host request. Exact-source extraction reproduced unsolicited output, producer exit, and then BrokenPipe on the request write. The candidate adds only a 256-byte `fgets` before `puts`, returning failure on EOF. Both original (`ok:true`) and replacement (`ok:false`) producers use the same bounded request consumption. There are no production changes, sleeps, retries, ignored host write failures, or weakened assertions.

## Exact proposal

Only `src/plugins/registry.rs` changes in the integration proposal. `registry.rs.base`, `registry.rs.proposed`, `base-map.json`, `candidate-map.json`, and `candidate.patch` provide exact bytes, hashes, and normalized patch. The unchanged release grant, post-verification installed inode mutation, and original-success assertion remain in place.

Private source is `target/agent-work/plugin-registry-fixture/app`; it copied all 696 source_inputs plus test files from current MAIN, including include assets. `base-source-inputs.json` and `base-test-inputs.json` pin the baseline. Private Cargo package version is `0.1.929`; normal binary name is `termcraft-plugin-registry929`. `candidate-source-inputs.json` and `private-build-delta.json` show only private Cargo identity plus the fixture change. Cargo identity edits are excluded from the payload.

## Deterministic protocol evidence

`probe_fixture.py` extracts the actual C string template from the supplied Rust file and compiles with `cc -Wall -Wextra -Werror`.

- `python3 probe_fixture.py registry.rs.base` exits 1 (expected RED): producer has exited before any request; write raises BrokenPipe.
- `python3 probe_fixture.py registry.rs.proposed` exits 0 (GREEN): no output or exit before request, successful write, original true response, clean exit.

The evidence probe uses a 250ms bounded readiness observation, not a sleep in the fixture/test. EOF and process wait provide RED completion barriers. The probe always reaps its child. Its BrokenPipe exception handling records only the expected RED observation; the host error behavior is unchanged.

## Rust verification

Every Cargo invocation ran from the private app with `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR` set to MAIN app target, and `cargo test --offline --locked --lib`. No own test executable was rebuilt while running.

Focused filter: `plugins::registry::lifecycle_regressions::registry_executes_granted_elf_when_installed_inode_changes_after_verification -- --exact --nocapture`.

- `focused.log`: initial focused test passed (1/1).
- `focused-repeat.log`, `repeat-summary.json`: emitted library test binary `harbor_workspace-3516e47b50d7b203` ran the exact focused test 30 times; 30/30 passed.
- `registry-lifecycle.log`: all three registry lifecycle tests passed.
- `counterexample.log`: temporary private-only production-path mutation reopened the installed executable after the verification hook. The actual replacement image then replied false. Existing assertion failed with `Err(Invalid("Plugin reported failure"))`, exit 101. This proves bypassing the granted frozen image is still caught. Exact temporary bytes and patch are `registry.rs.counterexample` and `counterexample.patch`; they are evidence only and excluded from the candidate patch.
- `restored.log`: after byte-for-byte candidate restoration and rebuild, all three registry lifecycle tests passed.
- `plugins-focused.log`: restored candidate passed all four plugin tests, including the previously repaired host fixture.
- `final-verification.json`: exact candidate restored; source inputs match the candidate inventory; MAIN registry still matches base; all owned child jobs completed and were reaped.

Repeat success is finite evidence; request synchronization supplies the causal fix. This task did not run a whole parent gate. Independent review, mechanical integration, and a fresh parent gate remain parent-owned.
