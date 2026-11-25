# Parallel SSH and Chromium fixtures

The original main-workspace gate logged two failures in
`../parallel-integration-platforms/full.log`. The SSH test connected its terminal
and timed out waiting for a shared SFTP snapshot. The pointer test timed out
before its first Chromium fixture frame.

The retained binary at the original path had since been replaced by a private
build. `retained-targeted.log` and `retained-parallel.log` record those preliminary
checks only. The diagnostic build used the main test sources. After finding
that its linked application library still used the private checkout's RDP
helper path, we rebuilt both the library and test executable from the main
workspace. The diagnostic build's browser engine, SSH client, agent and
shared-network reader sources matched the main workspace. We verified the
embedded RDP path before running the full GUI suite. Each binary was copied before subsequent builds could overwrite it.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --bin harbor-workspace --no-run
TERMCRAFT_REQUIRE_CHROMIUM=1 RUST_TEST_THREADS=16 taskset -c 6 \
  target/debug/deps/reliability-diagnostics ui::web_browser::tests owned_agent_selected
TERMCRAFT_REQUIRE_CHROMIUM=1 RUST_TEST_THREADS=16 taskset -c 6 \
  target/integration-fixture-bin/harbor_workspace-fixtures ui::web_browser::tests owned_agent_selected
TERMCRAFT_REQUIRE_CHROMIUM=1 target/integration-fixture-bin/harbor_workspace-main
```

Under CPU contention, browser diagnostics reported a closed Chromium session
that had timed out at `Target.getTargets` before the fixture received a page
request in `diagnostics-1cpu-targeted.log`. The same one-core subset failed again
in `red-1cpu.log`, where SSH did not reach listener readiness and Chromium
closed during `Page.getNavigationHistory`. A separate SSH run in
`guard-1cpu-targeted.log` reached `Refreshing` with one accepted public-key
authentication. The source worker therefore started after terminal
authentication and was still waiting for its own authentication. The full stress
test on two logical CPUs pinned both CPUs to one physical core and also broke
unrelated tests.

The four Chromium GUI fixtures and the owned-agent terminal/SFTP fixture now
reserve one shared test slot before starting their stage deadlines. The slot
covers the fixture lifetime. The rest of the suite runs in parallel. The tests
use their original stage deadlines. The SSH test checks one accepted
public-key authentication after terminal connection and two after the snapshot.
The pointer test waits for the current viewport paint containing the fixture's
red page before pressing, then continues to assert queue saturation and the
exact Chromium release and button state. Failure messages give the observed
stage without dumping credential forms, headers, or key material.

The same five-test one-core subset passed after the fixture changes, with
five passes and zero failures in `green-1cpu.log`. The initial default-parallel
rerun passed both affected cases and all four Chromium GUI cases. Its separate
RDP failures came from the private helper path embedded by the reused library.

The rebuilt main executable (`df47f2492fcdff637a2534c8151a0f88cbc352ad3b52a6805c996255e2855eda`)
passed 132 tests with zero failures and five existing ignores under default
parallelism in 49.22 seconds. See `main-parallel.log`. The same executable
also passed all five affected fixtures on CPU 6 in 23.45 seconds. See
`main-green-1cpu.log`.
