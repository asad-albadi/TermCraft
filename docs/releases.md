# TermCraft desktop release delivery

The [release workflow](../.github/workflows/release.yml) builds an **unsigned
core** candidate from the exact checked-out project commit. GitHub Actions
artifacts are test outputs. Only the [GitHub Releases page](https://github.com/asad-albadi/TermCraft/releases)
shows assets that have actually been published.

| Target | Candidate format | How it runs or installs |
| --- | --- | --- |
| Linux x86-64 | `.tar.gz` and Debian-family `.deb` | Extract and launch `harbor-workspace`, use the optional per-user desktop integration, or install the Debian package with the distribution's package manager. The hosted artifacts are built on Ubuntu 24.04. Read the package's glibc and graphics requirements rather than assuming compatibility with an older distribution. |
| Windows x64 | `.zip` | Extract and launch `harbor-workspace.exe`, or run the included `install-per-user.ps1`. Keep the adjacent RDP worker with the app. |
| Apple Silicon macOS | `.zip` containing `TermCraft.app` | Extract with Finder or `ditto` and move the bundle to the desired Applications directory. The deployment target is macOS 14.0. Gatekeeper may require explicit user approval for this unsigned bundle. |

Intel macOS and Android are outside this workflow. AppImage is not a
candidate format yet. The core includes the app, adjacent RDP worker,
desktop integration, policies and dependency notices. It does not include
optional private X servers, bundled Chromium, Mosh, the optional Wayland
sharing worker, or other separately qualified runtime packages. An installed
Chrome or Chromium provider is required by the macOS regression lane but
is not added to the release ZIP.

## Run and interpret the workflow

Use **Actions → Release artifacts → Run workflow** to test a commit without
publishing anything. Manual runs are read-only even if dispatched on a tag.
No signing secrets are needed for these tests. Unchecking `native_gui`
collects diagnostics only and intentionally blocks assembly. The optional
Windows X11, VNC input and Winspool fixtures are private diagnostics, not
public release assets.

For version-tag runs, use a canonical `v<major>.<minor>.<patch>` matching
the application Cargo version. Preflight also validates and records the RDP
and Wayland Cargo package versions independently. Every successful assembly
requires the three complete native regression jobs, native packages built
with Rust 1.96.1, native startup and two-version lifecycle evidence on all
three platforms, and a separate Linux native Wayland check. It then
revalidates downloaded packages and assembles one set of checksums. Failed
package jobs upload diagnostic evidence separately and never upload their
unverified `dist` contents as candidates.

The qualification workflow requires a relocated portable package to run with
an isolated workspace and empty executable search path, produce a nonblank
screenshot, and start its own RDP worker for a loopback negotiation without
losing the saved session or preferences. It runs the Linux package separately
under Xvfb (X11) and a private Weston headless compositor (native Wayland,
with no X11 fallback). Windows uses pinned software OpenGL. These are
desktop and package checks, not hardware coverage.

For lifecycle checks, the newer installed app must be the actual release
candidate. The workflow builds a lower application version from the same
commit in a separate private source tree, recording exactly what it changed.
That baseline is neither a published release nor a public asset. Linux tests
Debian installation, upgrade, reinstall and purge, plus a separate per-user
portable desktop installation. Windows tests side-by-side KnownFolder
installation, Start menu targets, reinstall and removal. Apple Silicon macOS
tests replacement, relocation and targeted LaunchServices registration and
removal. Each installed startup needs a fresh screenshot and workspace
preservation evidence. A separate record binds the exact launch arguments,
executable and newly created screenshot, even if the GUI emits no log text.

The synthetic fixture verifies stored sessions, preferences, shortcuts,
encrypted vault bytes and decryption through the Rust library, plus byte
preservation of recovery and malformed originals. It also scans both owned
workspaces for plaintext copies of the private, per-run synthetic values. This
is not a packaged GUI
unlock, recovery restoration, Finder or Gatekeeper approval test. Only a
hosted run with the required native checks and matching downloaded evidence
can qualify a candidate.

If publication becomes eligible, only a matching tag push can enter the
final job with GitHub release write permission. The publisher checks the
remote tag's commit and the exact asset names and hashes. An existing
published release with matching assets is a read-only success. A conflicting
or incomplete release fails and is not overwritten. A newly created draft is
checked before publication. A preexisting draft needs operator review. Signing,
notarization, wider runtime redistribution and device qualification remain
separate decisions. An unsigned core release does not imply any of them.

## Verify bytes and locate source

Check the downloaded `SHA256SUMS` (for example, with
`sha256sum -c SHA256SUMS`) against the four installer/portable assets,
`release-inventory.json` and the tracked project source archive. The
inventory records the source commit, archive digests, Cargo package
versions, toolchain strings, package manifest digests, terminal gate results,
and hashes of the per-platform startup, lifecycle and Linux Wayland evidence. Each platform’s dependency
notice inventory is resolved again from the checkout’s locked Cargo graphs
in the aggregate job. The manifest’s dependency identities, notice paths,
and notice bytes must match that independent inventory. Raw evidence stays in
the corresponding Actions candidate artifacts, not the public asset set.

`TermCraft-source-<commit>.tar.gz` comes from `git archive` at that exact
commit. The root includes `LICENSE`, `PRIVACY.md` and `CREDITS.md`.
`engines/rdp` is a checkout-relative source pointer in the runtime manifest,
not an unpacked directory in the binary package. The binary package's
`build-source/engines/rdp/Cargo.lock` pointer is relative to its resources.
The packaged lockfiles and build-input hashes are not the full project
source or a vendored dependency bundle. See dependency sources and notices
declared by the package manifest. Neither the source archive nor its
checksum is a reproducible-build claim.

## Keep workspace data

Run the extracted executable with `--version` to read the version compiled
into the app without opening a workspace. Close the app before replacing it. Program files are separate from the
per-user Harbor workspace. The defaults are
`${XDG_CONFIG_HOME:-$HOME/.config}/harbor` on Linux, `%LOCALAPPDATA%\Harbor`
on Windows, and `~/Library/Application Support/Harbor` on macOS. Removing the owned program or registration
does not intentionally remove that workspace. Back up the workspace before a
manual installation or migration. See the [packaging guide](../packaging/README.md)
for specific desktop installation and removal behavior.
