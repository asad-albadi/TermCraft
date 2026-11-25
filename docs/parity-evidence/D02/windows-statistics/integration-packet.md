# Windows serial statistics FIX1 integration packet

Read REPORT.md and COMMANDS.md for scope, exact validation and limitations.

- `fix1.patch`: three-file delta from the reviewed original candidate, context 10.
- `integration.patch`: cumulative eight-file patch from MAIN baseline, context 10.
- `payload/`: exact cumulative integration files (eight files).
- `reviewed-base/`: exact eight reviewed candidate files before FIX1.
- `base/`: retained cumulative baseline inputs (new files absent).
- `baseline-map.json`: original reviewed candidate SHA-256 map.
- `hash-map.json`: per-file MAIN/cumulative-base/reviewed/FIX1-source/FIX1-payload hashes.
- `source-inputs.sha256.json`: complete private build source hash map.
- `manifest.json`: changed-file list, identity and Linux helper binary hash.
- `order_fixture.py`: ownership ordering source proof.
- `verify_packet.py`: applies delta/cumulative patches and checks payload and MAIN hashes.
- `prior-*`: retained prior report, manifest, patch and seal; original directory untouched.
- `evidence.sha256`: FIX1 packet/log integrity seal.

No files in this packet have been applied to MAIN. Private identity files are excluded from integration. Ordinary data/control cancellation remains separately open. This packet awaits the parent's scoped review.
