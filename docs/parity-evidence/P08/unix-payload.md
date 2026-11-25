# P08 portable Unix release payload — private handoff

Date: 2026-11-24. Scope: optional Unix payload packaging only. No MAIN production changes, commit, release publication, or parity-point completion is claimed by this handoff.

Private source: `target/agent-work/p08-unix/`. The exact new-file ownership list, shared-file hashes and patch are in `handoff.json`, `basehashes.json`, and `shared.patch` there. The shared patch was rebased on MAIN after B05 and B03 merged; its `scripts/package-release.py` base SHA-256 is `ac1ff2052763febb7bec90a3c2f89b0fff6fbe9a4e821e98aee71c253cf6a235`. B05's native macOS launcher hooks remain present. P06's Mosh hooks remain unchanged. B03's Iperf argument, staging and notice hooks remain present; the composition test includes the real pinned Iperf runtime from MAIN.

## Behavior and invariants

`--unix-cache PATH` accepts the existing P05 upstream artifact cache keyed by SHA-256. It explicitly opts in and appends `-unix` to the release archive name. Linux x86-64 includes BusyBox 1.31.0 and 1.35.0; Windows x86-64 includes BusyBox-w32 FRP-6354. macOS and other architectures reject the option. Without the option, neither Unix archives nor their inventory are added. The option composes with the existing Xwayland and Mosh options.

The new `packaging/unix-runtime/stage-release.py` reads the unchanged P05 index and reuses the unchanged deterministic assembler. It verifies cached binary/source/configuration hashes, extracted license hashes via the assembler, the final assembled archive hash, and the native executable format. Linux headers must identify static ELF x86-64 without dynamic/interpreter segments; Windows headers must identify executable PE32+ x86-64. The helper replaces the generic assembler's downloader with a strict local reader, including after preflight: no download fallback is allowed. Missing files, modified files, symlink input files and unsupported targets fail closed. Neither packaging nor staging executes upstream payloads.

The matching archives are placed directly in `unix-packages/` beside the executable, matching `PackageRelease::bundled_archive()` in `src/packages/index.rs`. Corresponding source archives remain whole. The pinned 1.35.0 configuration is retained. The published 1.31.0 build script and complete x86-64 build log, including generated configuration, are additionally retained as pinned build information. Named source license/attribution files are extracted safely as regular files; all per-file notices remain in the exact source archives. Existing archive pins and runtime installer formats do not change.

`unix-packages/INVENTORY.json` and `PACKAGE-MANIFEST.json`'s `runtime.portable_unix` record versions, target, archive/binary/source/configuration hashes and locations, native format, source URLs, notice paths, build information and provenance limits. The normal package file inventory covers all added files. Frozen source capture includes the new helper and notice material through the existing packaging tree inclusion, without introducing target-tree copying. The six-asset allowlist, app/helper paths, build isolation and atomic no-overwrite publication remain intact.

## Notice inspection and exact limits

The cached BusyBox source releases contain the GPL-2.0-only `LICENSE` and `archival/libarchive/bz/LICENSE`; BusyBox-w32 also contains `win32/resources/COPYING_CCBYSA3`, attributing artwork to GNOME Project and referencing CC BY-SA 3.0 US. These actual source notice files are retained; all source-level comments remain in their corresponding source archives. The GNOME notice as supplied upstream references the full license URL; it is not itself the full CC license text.

The initial cache contains no musl notice material. The official Linux binary URLs advertise musl builds, and the verified binaries have static ELF headers. Added notice files retain actual upstream musl COPYRIGHT text and verbatim source comment blocks containing copyright, license, permission or public-domain language from the musl 1.1.22 and 1.2.2 releases. This includes TRE, crypt, math, sort and architecture notices, including architectures conservatively beyond x86-64. These are text, not README-only links. Their source URLs, reference source archive hashes and notice hashes are pinned in `packaging/unix-runtime/notices/manifest.json` and copied into the runtime inventory. Staging verifies the added notice hashes before inclusion.

Reference sources: [musl 1.1.22](https://musl.libc.org/releases/musl-1.1.22.tar.gz), [musl 1.2.2](https://musl.libc.org/releases/musl-1.2.2.tar.gz), [musl COPYRIGHT at v1.1.22](https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?h=v1.1.22), [musl COPYRIGHT at v1.2.2](https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?h=v1.2.2). The added [1.31.0 build script](https://busybox.net/downloads/binaries/1.31.0-defconfig-multiarch-musl/build-cross.sh) and [x86-64 log](https://busybox.net/downloads/binaries/1.31.0-defconfig-multiarch-musl/busybox-x86_64.log) show its static build/configuration but do not identify the exact linked musl release.

The BusyBox pins do not establish the precise musl version linked into either binary, a verified upstream toolchain, or a reproduced source build. Accordingly, inventory records `linked_version: null` for musl and labels both notice versions as conservative references, not verified linked versions. This work does not establish complete source-build reproducibility or a legal-compliance certification. Native Windows execution and actual TermCraft release startup remain parent qualification tasks. No Windows payload was executed.

## Verification

RED: the first valid-cache run against the original packager failed on the absent `unix_cache` API and missing release staging helper. See `unix-payload-red.log`. The absent-option baseline already passed.

Final command from workspace root (private source refreshed after B05 and B03 merged):

```sh
CARGO_BUILD_JOBS=2 \
TERMCRAFT_TEST_UNIX_CACHE="$PWD/app/target/p05-cache" \
TERMCRAFT_TEST_IPERF_ROOT="$PWD/app" \
python -B -m unittest discover \
  -s app/target/agent-work/p08-unix/tests \
  -p 'packaging*release.py' -v
```

Final result: 20 tests run, 19 passed, one optional real Xwayland runtime test skipped because its archive/ABI environment was not supplied. Full output is recorded in `unix-payload-final-tests.log`. New tests verify opt-in inventory, every output file hash, whole corresponding source, pinned configuration and build information, source/binary/config/archive/notice tampering rejection, wrong native format, unsupported target/architecture, missing and symlink cache rejection with network disabled, CLI option visibility, no-overwrite and unchanged cache, independent immutable frozen source, exact six assets, Windows ZIP layout and app/helper adjacency, and composition with Xwayland/Mosh.

The packaged app/helper executables are deliberately labeled layout fixtures: native small C executables on Linux and header-only PE fixtures for the Windows ZIP layout. Crate notice collection is mocked in those fixture package tests. Xwayland/Mosh composition uses labeled layout staging substitutes; their real payload qualification remains separate. The same combined package uses B03’s actual pinned Iperf2/Iperf3 payload, preserving their executable, source and license inventories; the Iperf payload is inspected rather than executed by packaging. The actual hash-verified Linux BusyBox `ash` from both assembled archives is executed with empty PATH and a private temporary home, reading a fixture file and printing the expected data. Upstream Windows binaries are inspected only. Existing packaging tests additionally verify relocation, menu ownership, preserving workspace data, source snapshots, Cargo configuration isolation and asset boundaries. The Cargo isolation check uses its existing temporary standalone fixture and independent target location, never the application crates. All Python sources were syntax-checked.

Parent must apply the owned new files plus narrow patch, perform fresh review, then build/qualify the actual release. No installed-user data is read or modified by these checks.
