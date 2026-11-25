# Macro fixture recovery

Scope: only `src/ui/macro_events.rs`, within `timed_terminal_macro_records_content_and_binds_replay_across_active_switch`. No production code changed. Checkpoint remains 66.7% (40/60).

## Cause

Original A and B are intentionally broadcast to both real owned PTYs. The old fixture waited for first-terminal AB only, then sampled the second terminal as the replay isolation baseline. A second terminal that had acknowledged only A could acknowledge its original B during replay, causing the unchanged-second assertion to report AB versus A despite replay targeting the first terminal correctly.

## Deterministic RED and gated GREEN

`macro-recovery-red.rs` is an exact diagnostic source snapshot. Only the second owned Python PTY fixture changes: it acknowledges A immediately, buffers B, and acknowledges B when an explicit ! control byte arrives. It uses no added sleeps or expanded deadlines. The old baseline is sampled at A, ! releases the original B, and the test waits for its actual acknowledgment before the unchanged isolation assertion. Thus the adverse scheduling order is deterministic rather than probabilistic. `macro-recovery-red.log`: exit 101, 0 passed / 1 failed; original assertion gets left [65,66], right [65].

RED command:
```
CARGO_BUILD_JOBS=2 cargo test --bin harbor-workspace timed_terminal_macro_records_content_and_binds_replay_across_active_switch -- --exact ui::macro_events::tests::timed_terminal_macro_records_content_and_binds_replay_across_active_switch --nocapture
```

`macro-recovery-gated-green.rs` retains that same transport gate, releases B and waits for both original acknowledgments before sampling the baseline. `macro-recovery-gated-green.log`: exit 0, 1 passed / 0 failed. Command:
```
CARGO_BUILD_JOBS=2 cargo test --bin harbor-workspace ui::macro_events::tests::timed_terminal_macro_records_content_and_binds_replay_across_active_switch -- --exact --nocapture
```

## Replay isolation sensitivity

`macro-recovery-broadcast-mutation.rs` starts from the final minimal fix and injects AB into the second real PTY at the replay boundary, explicitly awaiting that acknowledgment. This emulates the observable transport effect of wrongly broadcasting replay; it does not modify production dispatch code. The original unchanged-second assertion remains intact. `macro-recovery-broadcast-mutation.log`: exit 101, 0 passed / 1 failed; left [65,66,65,66], right [65,66]. Command is identical to gated GREEN above. The injected send and acknowledgment wait are removed from the final source.

## Final change and verification

`macro-recovery.patch` contains the complete final change: wait for first-terminal suffix AB and second-terminal exact AB before capturing the replay baseline. The existing 3-second condition deadline and 5-ms poll cadence remain unchanged. The existing recording timing delay remains unchanged. Recording content checks, active-tab switch and unchanged-second replay assertion remain intact.

`macro-recovery-suite.log`: exit 0, 10 passed / 0 failed / 0 ignored (140 filtered out), with final source restored and diagnostic gates/injection removed. Command:
```
CARGO_BUILD_JOBS=2 cargo test --bin harbor-workspace ui::macro_events::tests -- --nocapture
```

Final source SHA-256: `30fdde96081401e9e4f73e888a3c92fae66ce5c42c00b3d8e0020de1140b7aff`.

Full-suite verification is deliberately delegated to ROOT's fresh independent review/gate per coordination instruction, avoiding duplicate broad runs. All three scoped executable sessions and final suite session completed before handoff. No Git operations or normal production binary builds were performed.
