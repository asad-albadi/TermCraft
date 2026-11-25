# Packaged protocol runtimes

The optional Arch Linux x86_64 Mosh payload includes the pinned wrapper/client and its own verified Perl, OpenSSH, module and native-library dependencies. It requires the Linux kernel, glibc2.44, its OS loader and `/bin/sh`; it does not require installed Perl or an SSH client. Graphics requirements are separate.

Supply the exact Mosh archive, corresponding source and recipe declared in [runtime-manifest.json](../runtime-manifest.json), plus an offline cache containing the exact archive, signature, upstream-source and recipe inputs listed in [mosh-dependencies.json](mosh-dependencies.json):

```sh
python3 scripts/package-release.py \
  --mosh-archive /absolute/path/to/mosh-1.4.0-35-x86_64.pkg.tar.zst \
  --mosh-source /absolute/path/to/mosh-1.4.0.tar.gz \
  --mosh-recipe /absolute/path/to/PKGBUILD \
  --mosh-dependencies /absolute/path/to/verified-mosh-cache
```

The packager rejects missing or changed pinned inputs. It includes corresponding sources, original notices, authenticated package metadata and packaging recipes. The source/signature qualification and native dependency inventory are recorded in [the closure evidence](../../docs/parity-evidence/P01/mosh-dependency-closure/REPORT.md). Pins establish the approved bytes; they do not claim reproducible upstream package builds.

At launch, TermCraft verifies and copies the complete executable dependency closure into a private owned tree. The wrapper and its recursive SSH proxy use that private Perl/module tree and SSH client. Inherited loader/Perl injection variables are removed. Existing SSH authentication/configuration arguments remain intact; user keys, agents, custom external commands and hardware/authentication plugins retain their user/OS requirements. Unknown or changed host keys remain subject to SSH trust policy. The remote server still needs Mosh and UDP reachability.

Installed Mosh remains an explicit compatibility choice, including for existing profiles; missing bundled payloads never select it automatically. Combine the payload with `--xwayland-archive` when needed; see [the X server manifest](../x11/manifest.json). The embedded Linux browser has its own pinned Chromium package, source/notices and declared dependencies. Windows/macOS Mosh and Chromium runtime packaging remain qualification gaps.
