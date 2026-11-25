# Independent fixture packaging FIX1 review

Verdict: **SPEC ACCEPT; QUALITY ACCEPT for fixture-only FIX1.** No remaining scoped finding or required fix. Original actor/source review remains applicable; this document supersedes its packaging acceptance.

The parent integration check found a defect missed by the original review: tests/windows-serial-owner-component/main.rs was automatically discovered as an application integration test, causing root cargo fmt --all --check to resolve ungenerated completion.rs. This was a real integration defect despite the standalone generated fixture passing. The original report is preserved unchanged as requested.

## Scope and correction

Reviewed target/agent-work/d02-windows-owner-fix1 REPORT.md, integration-packet.md, patch, manifests, generator, README, preimages/hash map and covering evidence. The patch removes main.rs, adds byte-identical src/lib.rs, changes the standalone manifest entry point to src/lib.rs, and directs all five generated support artifacts to src/. README documents both the standalone generation requirement and corrected location. Cargo.lock is unchanged. Production serial_windows.rs remains SHA-256 9b13c1a1615f23c49e585d8004bf9111f58336d9d6f46812f0e9ba50e73be489.

Cargo root target discovery does not treat nested tests/<component>/src/lib.rs as an integration target. The component remains explicitly runnable through its own manifest. Its include!/path module references now resolve beside that entry point after generation, and generator default source-root resolution is unchanged. No root manifest, other test-target definition, or CI command change is required. The old main.rs must be deleted during integration; merely adding src/lib.rs would leave the original defect in place.

## Independent verification

- FIX1 evidence seal passed all 1,244 entries. Saved output in fix1-seal-check.log.
- Checked all six mapped preimages/final paths against base, payload, source and private root-view, including absent added-file preimage and absent removed-file final. Applied fix1.patch to a temporary baseline and verified every final hash/removal. The old main.rs and new src/lib.rs have the same 39e9c360d4f1de2f94828336373cb290a24a8c94781eadad02a2f54c549a30ca hash.
- Verified the product source's unchanged 9b13 hash in the FIX1 source tree. No actor or native behavior was re-reviewed or changed for this scope.
- Independently compared sealed red and green Cargo metadata inventories: 72 -> 71 targets; the only removed target is the unintended windows-serial-owner-component integration test. Every other target name, kind and crate type is identical. Both sealed Cargo 1.95/1.99 green inventories agree.
- Independently ran offline/locked Cargo 1.95 metadata against the private root-view with CARGO_BUILD_JOBS=2 and a unique review target directory; its inventory agrees with both green inventories. Independently ran root cargo +1.95.0 fmt --all --check there; exit 0. The root-view fixture src directory contains only lib.rs, with none of the five generated support artifacts, so this result does not depend on a prior fixture generation. See fix1-metadata.json and fix1-root-fmt.log.
- Independently regenerated the final payload fixture into this review directory, pointing it at the FIX1 source tree. All five generated artifacts in src/ match the sealed generated component byte for byte, and no old main.rs exists. Inspected the sealed standalone fixture result: 27 passed, 0 failed, 0 doc tests. A duplicate actor-suite execution was unnecessary for this byte-identical relocation.

No MAIN writes, app/helper builds, broad tests, native checks or root release/ Debian rebuilds were performed. The prior native Windows runtime qualification limitation remains unchanged. Apply this fixture-only patch mechanically on the accepted owner preimages, including its deletion, then retain the original actor acceptance plus this scoped packaging acceptance.
