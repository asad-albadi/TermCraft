# FIX2: required pinned-report structure and byte receipts

This bounded append-only delta addresses the remaining P2 in `../clean-FIX1/review.md`. No FIX1 source, raw report, sealed index or original 134-file evidence entry is rewritten. `predecessor-index.json` records the input identities; the wrapper compares those identities before loading FIX1. No report is fabricated as positive evidence, no historical data is resealed, and no GUI, Docker or full-suite replay was performed.

`source/pinned_iperf3.py` validates the **required pinned-report structure and byte receipts** for this fixture's iperf3 3.19.1, single-stream, forward TCP, 262144-byte client/server reports. This is not an exhaustive Iperf JSON schema or support for every version/mode.

Required observations now include:

- Nonempty, typed connection/start metadata with the pinned version/setup, loopback endpoints, fixture server port, matching session cookie and reciprocal data ports. The server's separate accepted control-connection port is validated as a port; it is correctly not mistaken for the data-stream port.
- Nonempty client and server interval/stream data, matching per-report socket identity, direction/omitted flags, numeric durations/rates/bytes, contiguous intervals and consistent stream/sum totals.
- Nonempty typed end-stream sender/receiver summaries with per-report socket identity, consistent durations/rates and stream/summary totals, plus matching client/server completed receiver receipts. The receiver-side server's unused sender total is correctly zero; actual client sender and server receiver totals remain262144 each.

Small timing/rate consistency tolerances account for the rounding visible in the actual retained full reports. Unrelated telemetry fields are outside this gate's documented guarantee.

`source/validate.py` overlays this parser onto the preserved FIX1 gate. Existing historical/evidence integrity checks and all required scenario/negative/browser/Unix/Iperf checks remain in force. The result explicitly states the scoped guarantee. Browser remains `SANDBOX_BLOCKED / UNAVAILABLE`; qualification remains incomplete.

## RED and GREEN

`RED-reduced-report.json` records the review's reduced client/server pair being accepted by the unmodified FIX1 parser, returning `(262144,262144)`. This is an adversarial input, not a claimed transfer.

`GREEN-parser-tests.log` covers the exact reduction, missing/empty required client/server structures, partial interval/end data, wrong connection/socket/session/setup/direction/type/duration/rate/byte values, and acceptance of the unchanged real full reports. Tests mutate only in-memory report copies.

```sh
PYTHONDONTWRITEBYTECODE=1 python docs/parity-evidence/V08/clean-FIX2/source/test_pinned_iperf3.py
PYTHONDONTWRITEBYTECODE=1 python docs/parity-evidence/V08/clean-FIX2/source/validate.py
PYTHONDONTWRITEBYTECODE=1 python docs/parity-evidence/V08/clean-FIX2/source/validate.py --require-complete
```

The observed diagnostic exit is0 with `VALID_PARTIAL_EVIDENCE` and actual sender/receiver262144 for Iperf2/3. Strict and optimized strict exits are2 because qualification is incomplete. `CLI-exit-contract.json`, `FIX2-observations.json` and retained stdout record these outcomes. The parser diff and source hashes identify the delta.

Native browser success, credential-vault upgrade contents, physical supported Linux distro/install/upgrade coverage, native Windows/macOS, full managed-API/runtime/original parity, and signing/notarization remain open. There is no V08 advancement.
