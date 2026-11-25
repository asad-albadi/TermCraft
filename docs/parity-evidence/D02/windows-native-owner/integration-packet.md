# Windows serial bounded owner integration packet

Review REPORT.md and COMMANDS.md first.

**Apply `owner-delta.patch` against accepted Stats FIX1.** Its six paths are one product source and five new test-component files. Seven accepted statistics paths in payload are unchanged and explicitly marked `in_owner_delta: false` in hash-map.json. Parent's separate Linux serial change is outside this candidate.

- `owner-delta.patch`: integration/review delta with ten lines of context.
- `cumulative-original-baseline.patch`: archival cumulative patch from the pre-Stats base; not the integration patch after accepted Stats FIX1.
- `reviewed-base/`: exact accepted Stats FIX1 eight-file payload.
- `payload/`: exact final thirteen-file cumulative payload.
- `baseline-map.json`, `hash-map.json`, `manifest.json`: exact base/delta/final relationships and private identity.
- `source-inputs.sha256.json`: complete private build inputs.
- `source/tests/windows-serial-owner-component/`: shipped exact-source controlled-native test component and its README.
- `owner-component.log`, `pure.log`, `windows.log`, `msvc-input-match.log`, `red.log`, `format.log`: covering evidence.
- `verify_packet.py`, `reconstruction.log`: patch application and exact payload verification.
- `prior-*`, `prior-seal-check.log`: accepted Stats FIX1 retained evidence, unchanged original seal.
- `evidence.sha256`: this packet and source seal.

No MAIN mutation was made by this worker. Independent review is owned by the parent. Windows hardware execution remains open.
