# Parallel desktop, legacy, serial and HTTP integration checkpoint

2026-11-24, Linux. This is implementation evidence, not native/original parity certification.

`final-full.log`: `CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast` exited 0. The last result of each of the 50 outer targets totals **479 passed / 0 failed / 6 ignored**. All 55 emitted summaries total 484 raw passes because five subprocess summaries repeat tests; do not compare that raw count as unique cases. `final-helper.log`: all **27 separate RDP helper tests** pass. Root/helper formatting checks pass.

The first boundary found an SSH fixture double-shutdown race (ENOTCONN); its targeted causal reproduction and fixed 15-case run are in parallel-gateway. The next boundary encountered the intentionally not-yet-exported new Tools module during parallel implementation; that compile setup error was resolved before the retained final gate. Neither was hidden by ignoring tests.

Implemented/reviewed checkpoint additions:

- D03: embedded VNC, verified VeNCrypt TLS and classic opt-in, Raw/CopyRect/Hextile, Rich Cursor, directional Unicode/Latin-1 clipboard, server resize, view-only/input policy, actual TCP and workspace rendering/input cases. See `../parallel-vnc/`.
- D04: shared embedded SSH forwarding for RDP/VNC/Telnet, independent gateway trust/authentication, immutable saved owner profile, original target TLS/NLA identity, authenticated private RDP helper rendezvous, certificate retry on the same SSH login, cancellation/cleanup, and CLI failure retirement. See `../parallel-gateway/`.
- P03: embedded Telnet/Rlogin/Rsh with real configured endpoint/user negotiation, Telnet NVT/BINARY Enter and echo semantics, ordered post-paste Enter, Rlogin urgent/window/flush, reserved-source-port behavior and bounded queues. Normal and isolated Linux namespace fixtures are recorded in `../parallel-legacy/`.
- P04: serial line controls, old defaults, PTY lifecycle/backpressure/unplug, write cancellation and Windows exclusive-open source compilation. Physical/native evidence remains pending; review fixes are in `../parallel-vnc/`.
- B02: optional ephemeral bearer token, read-only default, opt-in atomic new-file uploads, HEAD/ranges/ETag, confined paths, bounded/deadline-aware listings, asynchronous UI startup and private-token lifecycle. Hard-link publication is required; FAT/exFAT uploads remain unavailable. See `../parallel-http/` and this directory's real UI/controller test.

D02 additionally has explicit pinned Unix serial mappings with real protocol/PTY tests and stable binding across TLS retries; printer/smartcard, broader serial controls, identity-safe rename/delete and native/original coverage keep D02 open. P07 tools and P08 packages are still active work, not completed by this gate.

All native Windows/macOS, physical devices, external implementation interoperability and measured Personal 26.6 differences remain open unless separately evidenced. Checkpoints are 30/60 (50.0%); fully verified parity remains 0/60.
