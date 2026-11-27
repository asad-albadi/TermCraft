# Fixture family follow-up

CI45949734342 passed the corrected native Mac mouse, browser, SCP and serial checks, then exposed one broadcast startup assertion. CI45949731440 exposed one Linux X11 peer teardown assertion; the combined branch Linux job passed. These are fresh software regression gates; platform PRs remain unmerged until the next native run passes.

## GUI child readiness

The remaining six fixed-path Python GUI fixture launches now use provisioned PATH Python3 with isolated mode. A test-only helper polls each real child for READY and its required negotiated mode, using each caller's existing absolute two/three-second budget and per-child diagnostics. Broadcast children share the existing three-second deadline. Paste cancellation, stale-target and zero-payload assertions remain unchanged. Controlled helper checks reject READY without capability and accept the real capability; both actual broadcast Python children reached readiness over owned native PTYs locally. The precise Mac cold-start cause remains unproven pending native replay. Remaining frame/quietness loops are unchanged.

## Deadline peer teardown

The retry client's150ms deadline can expire while the simulated peer writes setup. A deterministic real Unix transport test held that peer behind a barrier until the client returned TimedOut, then reproduced the original BrokenPipe/join panic. The timed peer now accepts only specified disconnect errors after the original deadline and a main-thread-confirmed valid TimedOut result. Early disconnects remain errors. Successful generic setup callers stay strict. The same rule covers timed setup/header/payload/reply I/O.

The original140ms lower and300ms upper bounds remain, and retry counting now requires more than one complete initializer EOF rather than merely accepted sockets. A mutation extending the actual extracted production recovery budget by150ms failed. Twenty-one component tests using actual production transport/recovery sources passed five default-parallel runs after restoration. Component extraction avoided the known local full-GUI compiler/artifact problem; full hosted matrix execution remains required. No production code changes in this batch.
