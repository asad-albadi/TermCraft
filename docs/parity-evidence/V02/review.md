# V02 independent candidate review — 2026-11-25

Disposition: no important V02-specific source finding; Linux fixture source/evidence is ready for integration review. This does not complete V02 native/original validation. Shared V01 Dropbear staging/matrix findings are documented in `../V01/review.md`; they do not affect the V02 endpoint fixture.

## Inspected observations

Retained V02 final records: 11 passed, 2 unavailable. All mode/address-family rows select actual runtime assertions. Source exercises production `Tunnel::args` through `Process::spawn("ssh", ...)`, as used by the UI's launch override, with isolated batch auth and strict private known-host verification. No replacement embedded tunnel/GatewayLease implementation is invented.

Inspected assertions cover exact nine-byte TUNNEL_OK echo and observed peer/local addresses; local/remote/dynamic IPv4 and IPv6 forwarding; SOCKS5 no-auth CONNECT encoding and response; all modes bound to 127.0.0.2; Linux actual socket-table wildcard rejection; terminal forwarding diagnostics and process exit on occupied ports; channel close or explicit SOCKS refusal on a missing destination (read timeout cannot pass); closing Process with a live channel; listener rebind; and fresh launches twice using the same saved Tunnel config. Listeners and echo targets remain explicit loopback; fixture-only GatewayPorts clientspecified allows the selected remote bind.

Process Drop is the production lifetime primitive and meaningfully tests owned listener/channel cleanup. It does not prove a rendered tab/app close action, which README correctly leaves open. The reconnect assertion relaunches the saved Tunnel configuration and is not represented as a rendered workspace reconnect workflow.

## Completion boundary

Missing native Windows/macOS evidence remains unavailable, never passing. Actual rendered tab/app close and paired original Personal runtime behavior remain open. The source/test deliverable can be integrated after ROOT's boundary checks; strict V02 completion/checklist status must remain pending for these required observations. No count or checklist was edited by this review.

## Independent verification boundary

Reviewed the approved master plan, V01/V02 validation plan, parity-contract EvidenceRecord requirement and `docs/parity-evidence/schema.json`, candidate source, matrices, staging/provenance and retained results. This is review-only; no candidate source or production file was changed.

Independently checked: all 125 candidate-owned SHA256 entries match; all 125 MAIN base entries match (including expected absent files); all 8 retained executable copies match their manifests. The patch applies in a MAIN dry run (exit 0). Every final record has required fields, resolves its log relative to its evidence directory, and matches its log hash. Every passed record contains exactly-one-passed/zero-failed/zero-ignored output and the corresponding retained executable digest. Independently reran `python3 scripts/test_interop_runner.py`: 4 tests passed. No Cargo build or endpoint runtime suite was independently rerun; runtime assertions below are source-inspected and owner-run logs, not reviewer-run transport results. No full third-party JSON Schema validator was available/used.

The patch contains tests, helpers, scripts and evidence only; the existing SSH fixture change is opt-in stderr retention. Private Cargo identity changes are excluded. No production change or product defect is claimed.
