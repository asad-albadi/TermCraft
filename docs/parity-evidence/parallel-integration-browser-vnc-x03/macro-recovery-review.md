# Independent macro fixture recovery review — 2026-11-25

Verdict: **accepted**. Critical findings: none. Important findings: none. This is a test synchronization correction, not a product feature or parity-count change.

Independently verified the final MAIN `src/ui/macro_events.rs` SHA-256 is `30fdde96081401e9e4f73e888a3c92fae66ce5c42c00b3d8e0020de1140b7aff`, equals the saved final snapshot, and all 13 evidence-manifest hashes match. The before/final patch changes only the one test's pre-replay wait and its explanatory comment, inside the test module. No production dispatch behavior, deadline, recording assertions, active-tab switch or unchanged-second replay assertion was changed.

The original A/B broadcast reaches two independent real PTYs. Awaiting only terminal one's AB is not an acknowledgment of terminal two's B; taking its baseline at A can falsely attribute the delayed original B to replay. The final wait acknowledges both original inputs before taking that baseline. It does not relax the later isolation assertion.

Reviewed owner-run deterministic gated RED ([65,66] versus [65]), matching gated GREEN (one passed), deliberate replay-transport-effect mutation RED ([65,66,65,66] versus [65,66]), and final macro suite (10 passed / zero failed). The diagnostic gate/injection is absent from final MAIN. These are reviewed owner executions, not commands rerun by this reviewer. The mutation explicitly tests the observable transport effect rather than claiming a production-dispatch mutation. Root's fresh full gate remains necessary. No active commands remain from this review.
