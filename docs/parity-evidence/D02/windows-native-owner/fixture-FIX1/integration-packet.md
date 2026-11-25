# Owner fixture packaging FIX1 packet

Apply `fix1.patch` over the accepted/integrated owner candidate. This changes only fixture packaging: delete main.rs, add identical src/lib.rs, update manifest/generator/README. The Cargo.lock remains unchanged.

- `REPORT.md`, `COMMANDS.md`: scope, red/green evidence and exact commands.
- `fix1.patch`: scoped add/remove/modify delta.
- `base/`: exact five pre-fix fixture files.
- `payload/`: exact five final fixture files; no generated support artifacts.
- `preimages.json`, `hash-map.json`, `manifest.json`: remove/add hashes, unchanged lockfile and unchanged production serial_windows.rs pin.
- `source-inputs.sha256.json`: private source-input inventory.
- `root-view/`: private MAIN snapshot used for fresh metadata and root formatting; no generated owner fixture sources.
- `red-*`, `green-*`, `metadata-check.log`, `rename-proof.log`, `owner-component.log`, `product-unchanged.log`, `reconstruction.log`: covering evidence.
- `prior-*`, `prior-seal-check.log`: retained original owner report/manifest/seal; original evidence remains untouched.
- `evidence.sha256`: new packet seal.

No MAIN writes were made. Parent owns scoped re-review and mechanical integration.
