# Managed X server payloads

The Linux payload targets x86_64 with the Arch Linux runtime ABI and a Wayland compositor. Stage it in the extracted application folder, alongside the TermCraft executable, or select the same folder in **Servers → X11 server → Choose bundle folder**.

```sh
packaging/x11/build-linux.sh /path/to/application-folder
```

The script downloads the pinned Arch package over HTTPS. Pass that exact package as the second argument for offline staging. The script verifies the archive and executable independently. It copies the Xwayland binary, full upstream license, and Arch build and package inventories. See [manifest.json](manifest.json) for the source and build recipe pins and the shared runtime dependencies. The script stages files in the selected folder and requires an unused destination.

To include the payload directly in a native release archive, provide the pinned local Arch package to the release packager. This emits a separate `-arch-xwayland.tar.gz` release and records the engine, dependency inventory, license, and file hashes in `PACKAGE-MANIFEST.json`.

```sh
python3 scripts/package-release.py --platform linux --xwayland-archive /path/to/xorg-xwayland-24.1.15-1-x86_64.pkg.tar.zst
```

The managed engine opens a separate compositor window with the X desktop inside it. The compositor chooses its monitor. The engine's compile-time XKB locations resolve on the runtime system, so `xorg-xkbcomp` and `xkeyboard-config` remain requirements. The installed Wayland, Mesa, and other shared dependencies are also required. The application verifies the staged binary and executes a sealed in-memory copy through Linux procfs. It passes only two already-bound local Unix listeners to the engine. The runtime needs a kernel policy that allows executable memfds. The upstream package disables XDMCP. A native engine with XDMCP support will be needed for that feature.

`build-windows.ps1` and `build-macos.sh` extract pinned native upstream inputs into a separate candidate directory for qualification. They require native tooling and review before their payloads can enter an application build. The application currently reports these managed runtimes as unavailable.

- VcXsrv requires review of the PE/DLL imports, runtime redistribution notices, license coverage and corresponding source, and a native owned-display and authority test after Windows lifecycle support is implemented. The script reads the exact NSIS payload with 7-Zip.
- XQuartz still needs a private app-relative relocation of executable, dylibs, data, and frameworks, followed by signing and native tests on arm64 and x86_64. The script expands the upstream package with `pkgutil` and records the engine's current library references. It does not run package installation scripts or install system launch agents.

The source-backed private build path is separate from that inspection candidate:

```sh
TERMCRAFT_XQUARTZ_DEPS=/path/to/private-prefix \
bash packaging/x11/build-private-macos.sh \
  --source-archive /offline/XQuartz-xorg-server-source.tar.gz \
  --destination /tmp/termcraft-xquartz-private \
  --arch universal
```

It requires a native Xcode SDK and Meson/Ninja dependency closure, applies the pinned direct-entry patch through `prepare-source.py`, uses the authenticated-listener build flags, relocates non-system Mach-O dependencies, ad-hoc signs the staged tree, and writes `NATIVE-BUILD.json`. The resulting candidate remains unqualified until native lifecycle, authority, graphics, permission, and package evidence is recorded; it is not accepted by the compiled runtime trust gate automatically.

## Linux qualification

The ordinary Rust tests cover missing, invalid, and unsupported engine inputs. The live test runner requires Bubblewrap, a Cage compositor with Xwayland support, and Python 3. It isolates `/tmp`, network and process namespaces, uses a headless Wayland output, and gives the compositor a separate display :0. Use the executable path reported by the first command.

```sh
cargo test --offline --test xserver_lifecycle --no-run
tests/support/run_xserver_isolated.sh /path/to/application-folder \
  /path/to/xserver_lifecycle-binary --ignored --test-threads=1
```

The fixture replaces the compositor's Xwayland executable inside the namespace with the pinned payload. The test inspects the application worker's actual executable path separately. [Implementation evidence](../../docs/parity-evidence/X01/implementation.md) records the verified cases.
