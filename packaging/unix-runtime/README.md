# Portable Unix packages

TermCraft looks for pinned archives under `unix-packages/` next to its executable. You can also select an archive from **Tools → Portable Unix**. Select the matching release, choose an archive, and confirm the file name and published fingerprint. **Open Unix terminal** starts the installed BusyBox shell in its persistent private home. On Windows, use native drive paths such as `C:/Users`.

The package section supports switching versions, repairing the selected installed version, removing the active version and cleaning up retired files. Reconnecting a portable session uses the active release at reconnect time. An existing tab holds its release until it closes. Cleanup checks against the pinned release, leaves changed and additional files in place, and retains an ambiguously replaced file in a `.retained-*` directory inside its retired version. Portable home is outside the version directories.

## Assemble and verify

From the app directory, using Python 3:

```sh
python packaging/unix-runtime/stage.py \
  --output target/p05-bundles --cache target/p05-cache
cargo test --offline --test package_manager -- --include-ignored
cargo test --offline --lib packages:: -- --include-ignored
cargo test --offline --bin harbor-workspace portable_open_button -- --include-ignored
```

To stage checked inputs into an application artifact, use the distribution's executable directory followed by `unix-packages` as the output. Use `--platform linux` or `--platform windows` to stage only the target platform. Keep the corresponding source and notices with any redistributed binary archives. The script checks binary and source downloads against [the pinned index](manifest.json). It assembles deterministic USTAR and gzip archives, verifies each result and copies the upstream source archive and available build configuration alongside it. It uses an existing local cache when possible and performs no downloads or code execution as part of TermCraft's installation flow.

- Linux x86-64 uses published, statically linked BusyBox builds. Release 1.35.0 is first in the UI. Release 1.31.0 lets the same workflow exercise a version switch. Upstream publishes BusyBox under GPL-2.0-only. The official binaries are musl builds. [BusyBox release artifacts](https://busybox.net/downloads/binaries/) · [musl source and license](https://musl.libc.org/)
- Windows x86-64 uses the UTF-8 BusyBox-w32 FRP-6354 build with Windows system DLL imports. Its source tree includes the build configurations, GPL-2.0-only license and GNOME Project artwork attribution with CC BY-SA 3.0 US text. [BusyBox-w32](https://frippery.org/busybox/) · [Windows path conventions](https://frippery.org/busybox/paths.html) · [GNOME Project](https://www.gnome.org/)

The package is a user-permission shell with a private root, home and tool search path. The shell can access files permitted by the operating system. The current index covers x86-64 Linux and Windows. Native Windows conformance, a package catalog beyond BusyBox, and MobApt and `.mxt3` imports require additional qualification.


## Include in a release archive

First populate the hash cache using `stage.py` above. Native release packaging can
then include all pinned packages for the selected platform, without downloads:

```sh
python scripts/package-release.py --unix-cache target/p05-cache --output dist
```

This adds `-unix` to the release filename and places the matching archives in
`unix-packages/` beside the executable, where the Portable Unix UI finds them.
Linux x86-64 includes both pinned versions; Windows x86-64 includes BusyBox-w32.
macOS and other architectures reject this option. The option composes with the
Linux Xwayland and Mosh flags. Omitting it includes no Unix package archives.

`unix-packages/INVENTORY.json` and `PACKAGE-MANIFEST.json` record the archive,
upstream binary, source and available configuration hashes, retained notice
paths, native file format and provenance limits. Corresponding source archives
are retained whole. Named GPL, bzip2 and Windows GNOME attribution notices are
also extracted beside them; source-level notices remain in those source archives.
The published 1.31.0 build script and full x86-64 build log (including its generated
configuration) are retained as build information, and the separately published
1.35.0 configuration is preserved verbatim. Packaging never executes BusyBox.

Static Linux binaries are advertised upstream as musl builds. The added
`notices/` inputs preserve actual musl COPYRIGHT text and source-level third-party
license comments from the 1.1.22 and 1.2.2 upstream releases, with source URLs and
SHA-256 values in `notices/manifest.json`. These include TRE, crypt, math, sort,
and architecture-specific notices, retained conservatively for both binaries.
The linked musl version and upstream toolchain are not identified by the BusyBox
pins: these reference versions do not establish the exact linked version or a
reproducible build. Windows retains the upstream GNOME attribution/CC BY-SA 3.0 US
notice as supplied in its source; it references the full license URL. Packaging
checks PE x86-64 headers but does not qualify native Windows execution.
