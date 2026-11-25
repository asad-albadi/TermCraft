# FIX1: fail-closed clean Linux evidence validation

The three P2 findings in `../clean-review.md` are addressed in a separate append-only candidate. The original analyzer, all 134 indexed historical evidence files and immutable package bytes are unchanged. Their historical index identity is pinned to SHA-256 `3bd2371c13d7593cd1f81f85ee07d23ff707f7c32d85897af46dc2a0c3b1c0f4`; the new gate checks every historical file against that index, both immutable archives and all extraction hashes.

## Changes and actual evidence

1. `source/validate.py` requires all 13 historical scenario records and all six new scenario records (including the generation/CID cleanup probe), successful process/cleanup records, exact nonempty Iperf/Unix case counts and versions, actual Unix output, and the exact browser version. It compares an already sealed FIX1 evidence index rather than regenerating its own expected hashes. Missing, empty, duplicate/wrong-version, timeout, unsupported-exit and partial data are invalid.
2. Changed Mosh negative scenarios use a private signed Tesseract English OCR instrumentation image. Native screenshots are retained; OCR asserts the actual installed-provider-unavailable and bundled-provider-missing-Perl messages. The image checks that system Mosh and Perl are absent. Both minimal/full-runtime browser screenshots also have asserted actual UI failure text. Structured native browser outcomes assert loader return127/message in the minimal image, version return0 and exact158.0.8127.88 in the declared-runtime image, sandbox returnSIGABRT and exact diagnostic, and user-namespace denial with EPERM. Native browser remains **SANDBOX_BLOCKED / UNAVAILABLE**, never a positive runtime success. No sandbox bypass, privileged mode, host libraries/home/display/credentials/network or weaker security settings were used.
3. New direct Iperf checks use Iperf2 CSV and Iperf3 JSON client/server reports. The runner and independent gate both parse actual completed sender/receiver counts; all four observed counts are exactly262144 bytes. Metadata uses `actual_sender_bytes`/`actual_receiver_bytes`; requested counts are separate. Empty/truncated/partial reports cannot pass.

Only changed runtime/negative scenarios were rerun: Iperf, both Mosh errors, minimal browser, declared-runtime browser and OCR instrumentation. No full GUI/upgrade/install replay was needed. Existing reviewed historical positive Save/persistence/Unix evidence is retained and hash checked. Direct Iperf checks remain payload-byte observations, not managed-service API qualification.

The new runner captures its own container ID, validates its unique generation label/name before removing that ID, and has the original runtime restrictions/deadline. No other containers or images were touched. All owned containers and child processes exited and were reaped. The owned OCR image is retained with exact inspect/layer provenance, signed-package build log and complete version inventory.

## RED, GREEN and exit contract

`RED-old-gate-matrix.json` records eight independent falsifications accepted by the unmodified old analyzer in disposable copies: empty Iperf/Unix arrays, wrong Unix version, garbage browser version, missing/unsupported minimal-browser record, unsupported inventory exit and truncated Iperf server report. Originals were read-only inputs to these probes.

`GREEN-validator-tests.log` covers five test groups with many adversarial subcases, including each required record missing, exact counts/versions, malformed/truncated/raw partial transfer reports, incorrect UI text, wrong browser version/exit/timeout/status and edited/omitted sealed index entries.

```sh
python docs/parity-evidence/V08/clean-FIX1/source/test_validate.py
python docs/parity-evidence/V08/clean-FIX1/source/validate.py
python docs/parity-evidence/V08/clean-FIX1/source/validate.py --require-complete
```

The diagnostic command exits **0** only for valid **partial evidence**. The strict command exits **2**, including with Python optimization enabled, because qualification is incomplete. Invalid evidence exits **1**. These observed codes are retained in `CLI-exit-contract.json`; `FIX1-observations.json` explicitly has `qualification_complete:false`.

`FIX1-source.diff` compares old/new source; `evidence-index.json` seals the new observed raw files; `source-index.json` identifies reviewed source bytes. To rerun one changed scenario, use `source/runner.py NAME source/SCENARIO.sh --image IMAGE_ID`; the records retain the exact successful image IDs and expanded scripts. Rerun changes require a newly reviewed sealed evidence index; the validator never reseals for itself.

Native browser success, credential-vault contents across upgrade, physical supported Linux distro/install/upgrade matrix, native Windows/macOS, complete managed-API/runtime and original parity, signing/notarization remain open. No V08 or overall parity advancement follows from diagnostic exit0.
