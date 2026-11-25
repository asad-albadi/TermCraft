# Independent FIX1 review

Reviewed 2026-11-25, limited to this append-only FIX1 candidate. No production source or Docker resource was changed, and no GUI/full-suite replay was performed.

## Disposition

The three original P2 issues are addressed for the retained observations: all historical scenario records and exact expected runtime cases are required; native Mosh/browser error screenshots now have asserted OCR evidence and structured negative/blocked outcomes; actual Iperf sender and receiver byte counts are parsed and checked independently. The current evidence supports **VALID_PARTIAL_EVIDENCE**, with `qualification_complete:false`. One narrow P2 remains in the broader claim of strict raw-report schema/partial-data rejection, described below. No observed transfer count or other current historical result is disputed.

## Remaining P2: incomplete Iperf3 raw report structures pass the parser

`source/validate.py`, function `iperf3_bytes`, validates client intervals as only a nonempty list and otherwise reads three summary byte counts. It does not require meaningful interval contents, server intervals, connection/start metadata, or stream summaries. A direct isolated probe accepted the following pair and returned `(262144, 262144)`:

```json
client = {"intervals":[{}],"end":{"sum_sent":{"bytes":262144},"sum_received":{"bytes":262144}}}
server = {"end":{"sum_received":{"bytes":262144}}}
```

These are incomplete report structures, so “strict schema” and unqualified “partial reports cannot pass” in the handoff/tests overstate the gate. The existing sealed hashes prevent an unnoticed edit to the retained valid reports; this finding concerns the independent semantic check when reviewing a newly sealed rerun. Require the expected Iperf3 report fields and typed interval/stream data (including successful complete report structure and consistent connection identity), and add the above reduction plus a structurally partial server report to the adversarial tests. Alternatively narrow the documented guarantee to validation of present sender/receiver summary counts and JSON parse completeness. Current full raw Iperf3 logs and both Iperf2 CSV reports do record the expected completed 262144 bytes.

## Independently verified

- Ran `source/test_validate.py`: all five test groups passed, including required-record omission, invalid exit/type, wrong versions/UI text, edited sealed bytes, omitted sealed entry, malformed JSON and wrong byte counts.
- Ran the default validator under `python -O`: exit **0**, explicitly `VALID_PARTIAL_EVIDENCE` and incomplete qualification. Ran optimized strict `--require-complete`: exit **2** with the same explicit incompleteness. `need`/`integer` enforce gate requirements without relying on Python assertions. The retained CLI contract additionally records invalid evidence exit 1.
- Compared all **12 source-index entries** and **44 sealed FIX1 evidence entries** against disk: zero mismatches. Successful actual CLI integrity checks also rehashed the pinned original inventory, all **134 original evidence files**, both immutable archive digests and extraction manifests. No original candidate bytes were rewritten.
- Hash of `evidence-index.json` remained unchanged across both validator executions. Inspection confirms validation only loads/compares the expected index; it never reseals its inputs. Missing or additional top-level evidence files fail the index key-set check. Optional `--output` writes only a result, and no output is written by default. A future rerun still requires a separately reviewed new index.
- Historical GUI Save/persistence/PTY/Unix evidence remains pinned rather than silently replaced. All thirteen historical records and six FIX1 records are required, including cleanup and both inventories. The historical marker and raw logs are covered by the pinned original index.
- Inspected Mosh and browser OCR provenance and required exact named files/text; provider absence preconditions are explicit. Browser loader failure, exact packaged version, SIGABRT sandbox diagnostic, denied user namespace and no timeout are enforced. Native browser remains `SANDBOX_BLOCKED` / `UNAVAILABLE`; no success or fallback is inferred.
- Inspected new runner ownership cleanup: Docker creates the CID file, then cleanup inspects that immutable CID and verifies both the unique generation label and exact container name before removing that CID. Security/mount/network restrictions remain unchanged. No FIX1-labeled containers remained at review time. Review tests and processes completed; disposable test corpora were removed by their cleanup.

## Remaining qualification limits

Successful native browser sandbox, credential-vault upgrade contents, physical supported Linux distro/install/upgrade coverage, native Windows/macOS, complete managed-API/runtime and original parity, signing and notarization remain open. Diagnostic exit zero does not close V08 or release qualification.
