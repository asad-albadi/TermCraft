# Reviewed integration gate

Unfiltered Cargo command exited0: **554 outer-target tests passed, 0 failed, 15 ignored**, across58 targets. Separate helper suite: **39/39**. `result.json` and `source-sha256.json` describe this exact compilation boundary. Nested child summaries are excluded from counts.

This gate closes E02 following its four review fixes and the deterministic real-SSH startup fixture correction. It also covers the reviewed Linux Browser/X11 and printer slices. Those tasks remain open for their stated platform/provider gaps; D02 still needs smartcards. The prior full gate with its single SSH fixture failure remains in `../parallel-integration-macros-x11/` and is not relabeled as passing.

The Browser engine tests used real local Chromium HTTP/TLS/auth/proxy fixtures. Live X11 tests run separately inside private mount/network/PID/socket and nested-compositor isolation; extracted-release proof is under `../X01/`. No further host-display live tests are authorized.

Later runtime private-image staging/probe fixes and X02/smartcard development were not part of this source snapshot. Native Windows/macOS, physical devices and original-app runtime remain unverified.
