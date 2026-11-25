# Independent FIX2 residual review

Reviewed 2026-11-25. Scope: the residual FIX1 Iperf3 report-structure finding and new breakage introduced by the FIX2 overlay. No product changes, GUI/Docker operations, full-suite rerun, or new runtime images were needed.

## Disposition

**The residual P2 is addressed. No new actionable finding was identified within this bounded delta.** The new parser rejects the exact reduced report pair from the FIX1 review, requires the documented pinned forward-TCP single-stream report structure, and accepts the unchanged full actual reports with completed sender/receiver receipts of 262144 bytes. The handoff correctly limits its guarantee to required pinned-report structure and byte receipts rather than an exhaustive Iperf JSON schema.

## Evidence

- Independently compared all **59 predecessor-index entries** and **3 FIX2 source-index entries** against disk: zero mismatches. The predecessor preserves the prior source, raw reports, sealed indexes, handoff and review; the overlay checks these before importing FIX1. It does not edit or reseal predecessor inputs.
- Ran `source/test_pinned_iperf3.py`: all five groups passed. These cover the exact reduced client/server pair, missing/empty/partial required structures, endpoint/session/setup/socket/direction/type/duration/rate/byte mismatches, and acceptance of the retained full raw reports. Test mutations remain in memory.
- Inspected the parser requirements directly: pinned version/setup, reciprocal loopback data ports, shared cookie, typed connection/interval/stream/end metadata, explicit role/omission flags, duration/rate consistency, contiguous intervals and exact interval/summary completed-byte consistency. The server control peer port is treated separately from the data stream. Receiver-side server sender totals of zero are accepted correctly without reducing the required actual client sender and server receiver receipts.
- Ran the actual default FIX2 validator: exit **0**, `VALID_PARTIAL_EVIDENCE`, `qualification_complete:false`, both Iperf2/3 sender and receiver counts **262144**, and native browser `UNAVAILABLE: SANDBOX_BLOCKED`. The reported semantic scope is explicit. The underlying preserved FIX1 integrity/semantic checks still run, including preservation hashes for original evidence and immutable package inputs.
- Ran the actual optimized strict validator (`python -O ... --require-complete`): exit **2**, retaining explicit incompleteness. New checks use explicit exceptions rather than Python assertions. The wrapper replaces only the FIX1 Iperf3 parser and augments its result scope; it retains the earlier all-scenario, negative/browser, Unix and Iperf2 checks and exit contract.
- All review commands finished. No owned runtime jobs or containers were started; no review corpus or raw evidence was rewritten.

## Limits retained

This accepts the bounded validator improvement and retained partial observations. Native browser sandbox success, credential-vault upgrades, physical supported Linux distro/install/upgrade coverage, Windows/macOS, full managed-API/runtime/original parity, signing and notarization remain open. Neither diagnostic exit zero nor this review advances V08 or overall release qualification.
