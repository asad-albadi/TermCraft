# P08 packaging review

The two artifacts under dist match the saved SHA-256 checksums. The portable manifest identifies source snapshot `0a242a28146920d69d7fabac4c2f4b931aa02c698016c261ee32a2da84043a47`. Independent verification matched all 966 portable file hashes. Its assets directory contains two fonts, two notices and two images. All six Python packaging tests passed with `python -m unittest discover -s tests -p 'packaging_*.py'`.

## Important findings

- **Unintended asset publication.** At `scripts/package-release.py:48`, the snapshot includes every regular file beneath assets. At line 79 it follows file symlinks when copying. At line 336 the entire resulting directory goes into the distributable. A local draft or backup stored beside the logo therefore ships with the next release. The current release contains only the six intended assets. Specify those assets explicitly, reject symlinks, and use the same inventory for source capture and staging, including the prebuilt path. Add an unlisted benign asset fixture and check its exclusion.
- **Live Cargo configuration can change a supposedly frozen build.** At `scripts/package-release.py:418`, the default snapshot directory is inside the live workspace. At line 125 Cargo runs with that snapshot as its working directory. Cargo also searches ancestor directories for configuration. A new workspace-level Cargo configuration created after capture can therefore affect the compiler invocation, even though both checks of the frozen tree agree. Isolate Cargo configuration discovery from the mutable checkout and cover an ancestor configuration change with a real Cargo invocation on a tiny fixture.
- **The planned release gate is missing.** Task P08 at `docs/superpowers/plans/2026-11-23-07-protocols-tools.md:219` names CI and release workflows and a Cargo package-manifest test target. None exists in the application tree. The six standalone Python tests do exercise packaging but a normal Cargo test run skips them. Wire them into the release checks and provide a repeatable platform matrix. Keep signing and publication opt-in.

## Ownership, relocation and remaining evidence

The Linux registrar publishes a complete menu entry with no replacement and checks its content against the generating package before removal. Tests exercise existing entries, removal and separate workspace data. The Debian package owns system files and registers a symlink to the application beside its engine. Existing Wayland and X11 evidence covers relocated Linux startup and a loopback RDP negotiation. The Debian check extracted its payload. A clean-system package-manager install/upgrade/remove still needs native evidence.

Windows and macOS tests exercise synthetic archive layouts. They mock native dependency inspection. A Windows installer run, native macOS launch, clean-target runtime closure, signing and notarization remain open. The Windows uninstall ownership check at `packaging/windows/install-per-user.ps1:75` also accepts extra empty directories, which recursive removal at line 156 deletes. Preserve such directories or refuse removal when a directory is outside the manifest's ancestor paths.

## Review fixes and checks

- Source capture and package staging share an explicit six-file asset inventory.
  Tests verify that unlisted drafts stay outside captured source and release
  assets and that allowlisted symlinks are rejected.
- Cargo builds and dependency metadata use an absolute manifest path from a
  neutral volume root and a dedicated, configuration-free Cargo home. A real
  Cargo fixture fails when invoked normally beneath an injected live
  configuration and succeeds through the release invocation. The packaging
  cache keeps a stable path across builds.
- The Cargo `package_manifest` target runs all eight packaging regressions.
  The CI and manually dispatched unsigned release workflows have local YAML
  and Bash syntax evidence. Hosted executions remain pending.
- Windows removal rejects extra directories as well as extra files. Native
  installer execution remains pending.

[Cargo gate](review-cargo-gate.log) ·
[Workflow syntax check](review-workflow-check.log)

### Independent follow-up verdict

Eight packaging regressions passed in 3.55 seconds on Linux. The six-asset allowlist is shared by capture and staging. Cargo builds and metadata use the isolated invocation. The additional Windows directory ownership check and both workflows are present.

One important release-gate issue remains. The Windows matrix job runs the Cargo packaging test before building. At `tests/packaging_release.py:264`, the cross-platform fixture requires the host ZIP writer to record Unix mode 0755 for both synthetic targets. At `tests/packaging_debian.py:57`, the Debian fixture likewise requires 0755 in a tar created on the test host. Windows chmod only manages the read-only attribute, and the packagers record host file modes. Run the Debian fixture on POSIX and restrict POSIX-mode assertions to applicable hosts, or explicitly set portable modes in the archive writers. The release gate should still exercise the Windows layout on Windows.

### Platform fixture correction

The Debian fixture runs on POSIX, where it verifies executable modes and an
absolute launcher symlink. The native ZIP fixture checks executable modes for
both the application and helper on POSIX hosts. Its executable byte, bundle,
icon, and Windows installer manifest checks also run on Windows. All eight
Linux packaging regressions passed after the correction. Native Windows
workflow execution remains pending.
[Platform fixture results](review-platform-gate.log)

### Windows fixture gate resolution

The Windows permission finding is resolved. At `tests/packaging_debian.py:13` the Debian tar-mode and symlink fixture now runs on POSIX hosts. At `tests/packaging_release.py:265` only the ZIP mode assertions are POSIX-specific, and both executables are checked there. Windows layout, contents, icon and installer hash checks remain active on Windows. The platform-gate log records eight passing tests on Linux. Hosted Windows execution remains pending.
