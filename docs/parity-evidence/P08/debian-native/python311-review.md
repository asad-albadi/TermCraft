# Independent Python 3.11 packaging review

Decision: ACCEPT the narrow seven-file candidate for application and subsequent native qualification. No blocking correctness or security findings were identified. This acceptance is source-review and focused-test acceptance, not a claim that a genuine native release portable archive has completed Debian wrapping, installation, or startup.

## Reviewed identity

Private candidate: `target/agent-work/python311-packaging/candidate/`.

- Base normalized SHA256: `9901ab189809173f8f6956287d10b172ebb3bdb1205528bc2a0f13b791eef1dd`.
- Candidate normalized SHA256: `b331e30d66fc471815cebd6f5fcddc3bce0d221d4cd1bc40423369d2cfbc4faf`.
- Independently recomputed each of the seven base/candidate file hashes and both normalized hashes. MAIN's seven paths still matched the base at review time.
- Applied `candidate.patch` to a temporary copy of the seven baseline paths and compared every resulting file byte for byte with the candidate: exact match.
- Independently checked all 563 entries in `candidate-source-inventory.json` against candidate bytes.
- No application source changes were made by the reviewer.

## Important observations

The original unsupported `extractall(filter="data")` call is removed from both production TAR consumers. Extraction now uses only Python 3.11-compatible operations, without a permissive fallback.

Before payload writes, the helper validates canonical POSIX names, rejects absolute and traversal paths, Windows-style names, duplicate normalized names, files used as ancestors, all link/device/FIFO/contiguous types, and GNU/PAX sparse representations exposed by tarfile. Only explicit regular-file and directory types are accepted. Member data is copied to exclusive new files in a private sibling staging directory. Ownership/timestamps are not imported; executable files retain useful execute permissions while special bits and group/other write bits are stripped. Directories become 0755. Existing nonempty destinations and unresolved symlink ancestors/destinations are rejected.

The private staging tree is cleaned on validation/read/write failures; replacement publishes the fully extracted tree only after extraction succeeds. The caller must provide an existing trusted/private parent, as the helper documents. This is the actual usage in Debian wrapping and startup smoke. The repeated path checks are not a descriptor-based defense against a hostile same-user process concurrently replacing an ancestor; acceptance does not extend to a shared attacker-writable parent. The startup smoke's resolution of its own trusted TemporaryDirectory root correctly accommodates macOS `/var` aliases without relaxing arbitrary extraction-path checks.

Transaction boundary: archive extraction publication precedes Debian root/manifest/release-gate verification. A rejected manifest can leave an extracted tree inside the caller's temporary workspace until that enclosing workspace is cleaned. No Debian artifact is published on those failures. This matches the existing wrapper boundary; tests should not be interpreted as asserting manifest rejection rolls back the extraction helper's publication.

Exact sibling file-location imports work without relying on the current directory or global import search paths. The release source inventory now includes both the helper and startup checker. The frozen-import test verifies the helper's actual code filename originates within the frozen snapshot. Existing Linux/release gating, manifest comparisons, required programs, and Debian library allowlist remain intact.

## Independently executed verification

Host Python 3.14.10:

- `python3 tests/packaging_archive.py`: 8 passed.
- `python3 tests/packaging_debian.py`: 1 passed.
- `python3 tests/packaging_release.py`: 9 run, 1 existing skip, otherwise passed.

Pinned Bookworm image `sha256:1e7cd41f77c63d2093907d8a33cc65562eafd5cfbed60c829b9faedf652f24a0`, actual Python 3.11.2:

- Same archive suite: 8 passed.
- Same Debian suite: 1 passed.
- Same release suite: 9 run, 1 existing skip, otherwise passed.
- Both `scripts/package-debian.py --help` and `scripts/package-check-startup.py --help` exited successfully, checking direct CLI imports alongside the suites' file-location imports.

The review container had read-only candidate mount and root filesystem, networking disabled, an executable private `/tmp` tmpfs, unique name `termcraft-python311-review-independent`, and label `termcraft.task=python311-review-independent`; it was run with `--rm`. Final label-filtered inventory was empty. No builds or unrelated Docker resources were touched.

One initial probe omitted explicit `exec` on the `/tmp` tmpfs and produced two fixture `ldd` errors because fixture binaries could not execute. The complete rerun with executable `/tmp` passed; these initial environment errors are not candidate regressions.

## Scope and remaining evidence

The malicious-input suite meaningfully exercises both production consumers, mode handling, symlink ancestry, nonempty destinations, injected write failures, truncated archive errors, manifest mismatch, and frozen-source loading. The truncated archive fixture may fail during header preflight rather than after staging begins; cleanup during payload copying is directly covered by the injected write failure. The test named for missing manifests does not itself omit the manifest file, so its evidence supports missing required program/hash rejection rather than a dedicated absent-manifest fixture. Neither limitation blocks this focused compatibility/security change.

Previously recorded optional Unix Python 3.11 checksum failures were not broadened into this review or used to change pins. Their separate baseline evidence must remain distinguished from this patch's green core suites.

Outstanding root-owned qualification remains mandatory: use a genuine release-profile portable artifact from the native locked build, apply this accepted source change, then exercise the real Debian wrapper, package inspection/installation, and startup. The supplied prebuilt/unverified native archive's release-gate rejection remains correct and does not satisfy that positive qualification. No waiver is implied by this review.
