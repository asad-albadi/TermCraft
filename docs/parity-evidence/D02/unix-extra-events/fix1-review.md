# Scoped FIX1 rereview

Prior P1: **ADDRESSED** by removing unsupported modem-event promises, rather than attempting implicit native arming.

SPEC verdict for the approved narrowed delta: **PASS**. QUALITY verdict: **PASS**. No new actionable defect identified in this delta. This does not complete D02 or replace the parent's combined integration/release verification.

## Correction inspected

`FIX1/payload/engines/rdp/src/serial.rs` defines Linux COUNTER_EVENTS as 0xc0; the accepted family with RXCHAR is 0xc1. Worker support checking therefore rejects CTS/DSR/RLSD/RING individually and in every mixed mask before querying or committing an origin. Admission uses the same family constant and bitwise exclusion predicate, so modem-only and modem/supported masks cannot spend the reset escape slot. Non-Linux COUNTER_EVENTS remains zero.

The event snapshot is consistently reduced to four elements [native frame6, overrun7, parity8, brk9]. Observation maps index3 to BREAK and indices0–2 to ERR. No modem field contributes to event generation. No arming, thread, signal, carrier/flow mutation or cleanup requirement was introduced. Existing transactional SET, pending-wait retirement, fresh origin, RXCHAR gating and failure paths remain unchanged by the correction. The old query-success gate is no longer used to promise modem-edge recording, regardless of subsequent configuration changes.

The remaining receive-counter rationale is supported by the reviewed primary [8250 receive implementation](https://github.com/torvalds/linux/blob/master/drivers/tty/serial/8250/8250_port.c): serial8250_read_char increments break/parity/framing/overrun counters before applying read_status_mask and inserting the received character. This path does not use the modem-status MSI gate that caused the original P1. That is source evidence for the retained mapping, not a claim that physical errors were generated or every adapter was validated.

## Verification and scope evidence

- Independently checked all FIX1 HANDOFF.json hashes and all candidate-hashes.json entries, including sources and exact build executable artifacts. Payload equals the compiled private source byte for byte.
- Reverse-applying delta.patch reconstructs the exact sealed predecessor payload. Reverse-applying cumulative.patch reconstructs the exact original baseline serial.rs hash. Both product patches contain only serial.rs; private identity changes are excluded.
- Independently rechecked every predecessor HANDOFF entry: preserved reports, payload, patches, logs and seals remain intact. This review did not edit MAIN or the original review report.
- Inspected the policy RED log: predecessor accepted CTS after overridden counter-query success where UNSUPPORTED was expected. Inspected final GREEN log: private package termcraft-rdp-unix-events983, 82 unit and 10 PCSC integration tests passed, zero failures/ignored/filtered. No broad tests rerun.
- Inspected the expanded policy test: real test-owned PTY c_cflag is varied through all four CLOCAL/CRTSCTS combinations; each modem bit is rejected in worker and admission; rejection leaves flags unchanged. Its successful counter query is an explicit native-boundary fixture, not proof of physical MSI behavior.
- Existing revised tests retain fresh-origin, independent/combined BREAK/ERR mapping, wrap, between-wait retention, failure preservation and retirement, close, unrelated-field exclusion, and real PTY read coverage. Mixed supported/modem SET rejection preserves a live accepted wait. The admission predicate itself excludes mixed masks through the same bitwise test even though the new policy test enumerates individual modem bits.

The FIX1 report correctly supersedes prior modem-success claims and labels successful BREAK/ERR observations as controlled native-boundary fixtures. Actual unoverridden PTY refusal and real PTY data flow remain separately identified. Counter reset/full-cycle ambiguity is now explicit. No new physical UART, native RDP, Windows/macOS or original-program comparison evidence is claimed. Architectural unsupported controls were not reopened in this scoped review.
