# Building TermCraft release archives

Run the packager from the TermCraft source tree on the target operating system.
It captures a content-addressed copy of the release inputs, builds both Cargo
manifests with their lockfiles, and stages a versioned archive. The manifest
records the snapshot digest. Active workspace edits can continue while Cargo
builds the captured source. Use `--build-root` to configure the cache location. It prints the archive's SHA-256 and leaves any
existing archive intact. The package contains the application, the adjacent
headless RDP engine, artwork, font notices, crate notices, and a manifest with
binary hashes and build input hashes, alongside both lockfiles. Crate metadata
identifies upstream sources where published archives do not include notice text.
The Windows ZIP includes and hashes the project MIT license and privacy policy.

```sh
python3 scripts/package-release.py --output dist
```

Python 3.11+ and Rust/Cargo are required. Set `TERMCRAFT_OFFLINE=1` to restrict
Cargo to cached crates. The packager builds Linux tar.gz, Windows portable ZIP,
and macOS ZIP containing a Finder application bundle. Linux needs `readelf`
and `ldd`. Windows uses the MSVC Rust target and needs `llvm-objdump`, `objdump`, or Visual Studio's
`dumpbin`. macOS needs Xcode's `otool`. Windows and macOS additionally need
Pillow for native icon generation. The Windows x64 candidate has native build,
portable and installed startup evidence. Clean-account upgrade, optional
runtime coverage and macOS qualification remain pending.

To stage trusted prebuilt app and helper binaries, supply both
`--app-binary` and `--helper-binary`. Build them with `cargo build --release`
on the target platform. The prebuilt manifest records the packaging source and
labels binary provenance as prebuilt. That label indicates that the compilation inputs are unverified. Debug builds have developer-only RDP helper discovery.

On Linux, use a build environment matching the oldest intended distribution.
The package relies on the user's graphics stack, certificate store and listed
shared libraries. The manifest includes the highest glibc symbol version
required by each executable. See the platform requirements in the archive
for libraries opened dynamically and external clients required by optional
compatibility workflows.

Verify an archive on the target platform with Python, Pillow, and a working
native display. A headed session may briefly show the screenshot window.

```sh
python3 scripts/package-check-startup.py dist/TermCraft-0.1.0-linux-x86_64.tar.gz \
  --evidence docs/parity-evidence/parallel-packaging/startup
```

The checker confirms the package's file hashes, relocates it to a directory
with spaces, launches the packaged app and RDP worker with an empty PATH,
and checks a GUI screenshot. It opens a saved RDP profile against a
loopback fixture and checks that the workspace stays intact. It uses isolated
workspaces and leaves the installed user's settings intact. Supply an existing X server with
`--display` when checking Linux outside a Wayland session. Evidence from the
build host is distinct from validation on a clean target system.

Linux menu registration is optional. The package includes its install and
uninstall commands. The Windows ZIP supports portable launch and a built-in
PowerShell installer with a versioned per-user program directory and Start
menu shortcut. The [Windows x64 qualification checkpoint](../docs/parity-evidence/execution/2026-11-25-windows-native.md)
covers an unsigned candidate's relocated startup and versioned installer
smoke. For a local candidate, run `python scripts/package-check-startup.py <zip> --evidence <new-directory>`
and `pwsh -File tests/windows-package-lifecycle.ps1 -Archive <zip>`.
The installer test creates and removes uniquely named per-user versions.
On macOS, extract using Finder or `ditto` to preserve executable
permissions. The bundles are unsigned. Move the app to the chosen Applications
directory, and use the same location or a new versioned location for upgrades.

Workspace data remains in the existing Harbor
per-user data directories under XDG_CONFIG_HOME, LOCALAPPDATA, or Library
Application Support. Close TermCraft before upgrading and update any shortcuts
that refer to the old version. The workspace remains in place when you remove the program files.

## Debian package

Create a Debian-family binary package from the locked Linux release archive.
The package owns `/opt/termcraft`, `/usr/bin/termcraft`, and the system menu
entry and icon. Its Depends field records the required glibc symbol baseline
and graphics libraries. Build the portable archive on the oldest Debian-family
system you intend to support. Install or remove with the distro's package manager
on that system. Workspace settings stay in their existing per-user directories.

```sh
python3 scripts/package-debian.py dist/TermCraft-0.1.0-linux-x86_64.tar.gz --output dist
```

The build is deterministic for a given portable archive. It uses the Python
standard library and needs neither a global package installation nor a Debian
toolchain.

## Release checks

Run `cargo test --locked --test package_manifest` to exercise packaging
contracts through Cargo. The release builds use a neutral working directory
and isolated Cargo configuration. They reuse registry and Git caches where
linking is available. Cargo configuration and Rust build-override environment
variables from the live checkout do not apply. The native toolchain and
system SDK come from the build machine.

The CI workflow runs the Rust and packaging tests. The manually dispatched
release workflow builds unsigned archive candidates for Linux x86-64,
Windows x86-64, and both macOS architectures. Linux startup uses a virtual
X11 display. Windows and macOS GUI checks require a runner with an available
desktop session. The workflow retains run artifacts and startup evidence.
Signing, notarization, and publication are separate release actions.
