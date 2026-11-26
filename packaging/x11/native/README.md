# Private native X server candidates

Apple Silicon macOS can run the exact private windowed XQuartz tree pinned in
`macos-arm64.sha256`. The hash check covers the executable, dylibs, fonts, XKB,
bundle resources and corresponding source. The private Windows runtime
remains unavailable. A local JSON file cannot change these compiled trust
decisions. `source-contract.json` tracks inputs, toolchains and open evidence.
`upstream-dependencies.json` records the original PE and Mach-O inspection.

Pass the verified candidate directory to the macOS packager using
`scripts/package-release.py --xquartz-runtime CANDIDATE --output DIST`. The
packager copies only pinned files and verifies the staged signatures. A local
package can exercise the runtime without changing the user's XQuartz
installation. Public distribution approval is tracked separately.

Prepare either source tree with the pinned **local** source archive:

```
python3 packaging/x11/native/prepare-source.py --platform windows --source-archive /offline/vcxsrv-source.tar.gz --destination /unused/private-vcxsrv-source
python3 packaging/x11/native/prepare-source.py --platform macos --source-archive /offline/XQuartz-xserver-source.tar.gz --destination /unused/private-xquartz-source
```

This validates the archive, safely extracts it, applies the exact private patch, retains corresponding source and records every prepared regular-file checksum. It does not download, install, execute an upstream installer, change a daemon or modify an existing display.

## Windows native build recipe

Use a Windows x86_64 native worker with the upstream pinned source's `HOW_TO_BUILD.txt` prerequisites: Visual Studio 2022/VC143, the Windows SDK, Cygwin tools, Strawberry Perl and Python lxml/mako. In the freshly prepared tree, upstream's `buildall.sh 1 2 R` builds the 64-bit release with two jobs; its final NSIS step only produces installer archives, which must never be executed. The server output is `xorg-server/obj64/servrelease/vcxsrv.exe`. The DLLs, fonts, xkbcomp and xkbdata must come from that same native build and have their own complete pinned inventory. Do not combine an arbitrary rebuilt executable with PATH DLLs. Preserve corresponding source, this patch, exact SDK/tool versions, compiler options, PE import inventory and MSVC redistribution notices. Upstream scripts have not been run here: Windows native build/SDK tooling is unavailable.

The transport patch changes listener binds to IPv4/IPv6 loopback, replaces Windows SO_REUSEADDR with checked SO_EXCLUSIVEADDRUSE, and rejects a missing explicit display or displayfd scanning. The Rust provider reserves exactly one OS-selected loopback port, creates a suspended process with no inherited handles, assigns it to a kill-on-close job, releases the reservation, resumes, checks IP Helper ownership of the exact listener and requires anonymous rejection plus cookie-authenticated setup. A collision fails without scanning or altering the competing server. Snapshot PE/DLL handles deny write/delete sharing for the entire lease. The old upstream checksum is never an approval for this changed output.

## macOS native build recipe

A native worker needs the Xcode macOS SDK, Meson, Ninja, pkg-config, Bison,
Flex and MIG. Use the distribution gitlinks in `source-contract.json` for
xorgproto, Mesa, xkbcomp, XKB data and the core bitmap fonts. Mesa 22.2.5's
configure still uses Python's `distutils`. Python 3.11 with Mako provides a
working build environment. Set both `TERMCRAFT_XQUARTZ_DEPS` and
`PKG_CONFIG_PATH` for the private dependency prefix. Install the font
data with an explicit `FONTROOTDIR` under that prefix, since font-util can
otherwise select its own absolute directory.

The builder compiles the patched server and stages its generated plist, nibs,
defaults, xkbcomp, XKB data and fonts into a fresh tree. Upstream's XQuartz
installer hooks request root ownership. The builder skips these hooks and
stages the built files directly. To collect bounded desktop evidence, run
`python3 packaging/x11/native/probe-macos.py CANDIDATE EVIDENCE_DIRECTORY`.

Relocate every non-system Mach-O dependency and install name to app-relative `@loader_path` paths, verify the arm64 slice and dependency closure, and sign leaf dylibs/frameworks followed by the private app. Snapshot and pin the final signed tree including resources and code-signature files. The approved profile uses the exact inventory from this private build.

The direct-entry source patch bypasses Mach registration/startx/shell startup, requires our inherited listening AF_UNIX descriptor, disables all engine-created listeners/locks, and adopts it with `ListenOnOpenFD(fd, 0)` so Xauthority remains mandatory. The stock handoff uses `noxauth=1` and is unsuitable. XKB output stays inside the private runtime; default plist lookup is app-relative and authentication overrides are volatile Objective-C `NSUserDefaults` values. The provider starts a new owned process group; macOS `waitid(WNOWAIT)` preserves its identity until owned helpers are terminated and the leader reaped. The prebound socket lives only in the private directory. Existing displays, launch services, XQuartz preference domains and shell startup files are untouched.

Composite remains explicitly disabled under the upstream XQuartz 2.8.6 policy.
The authenticated macOS probe reads X11 geometry and GLX and RandR protocol
versions. The windowed profile leaves display mode and monitor selection to
macOS.

### Direct GLX qualification client

Use the source tree extracted from the pinned candidate's Mesa archive and
the pinned private X.Org prefix. The separate fixture records direct
framebuffer readback without changing the engine inventory. Use the Python
3.11/Mako environment described above with Meson, Ninja and Bison on `PATH`.

```sh
export SDKROOT="$(xcrun --show-sdk-path)" MACOSX_DEPLOYMENT_TARGET=14.0
export PKG_CONFIG_LIBDIR="$DEPS/lib/pkgconfig:$DEPS/share/pkgconfig"
meson setup "$BUILD" "$MESA_SOURCE" --buildtype=release --prefix="$MESA_PREFIX" \
  --wrap-mode=nodownload -Dc_std=c11 -Dosmesa=true -Dllvm=disabled \
  -Dvalgrind=disabled -Dzstd=disabled -Dc_args="-I$DEPS/include" \
  -Dcpp_args="-I$DEPS/include"
meson compile -C "$BUILD" -j 2
meson install -C "$BUILD" --no-rebuild
mkdir -p "$FIXTURE/opt/X11/bin"
xcrun clang -arch arm64 -Wl,-headerpad_max_install_names -Wall -Wextra -Werror \
  -isystem "$MESA_PREFIX/include" -isystem "$DEPS/include" \
  -L"$MESA_PREFIX/lib" -L"$DEPS/lib" packaging/x11/native/direct-glx-macos.c \
  -lGL -lX11 -o "$FIXTURE/opt/X11/bin/direct-glx-macos"
python3 packaging/x11/native/relocate-macos.py "$FIXTURE" arm64 --glx-probe
ditto "$FIXTURE" "$RELOCATED_FIXTURE"
python3 packaging/x11/native/probe-macos.py "$CANDIDATE" "$EVIDENCE" \
  --glx-client "$RELOCATED_FIXTURE/opt/X11/bin/direct-glx-macos"
```

Use `--allow-software-glx` for a guest without a compatible accelerated CGL
pixel format. Check `glx.log` for the actual renderer. The same owned socket,
cookie and cleanup checks apply on both paths. See the
[graphics evidence](../../../docs/parity-evidence/issue-2/xquartz-build.md#direct-apple-glx-on-apple-silicon).
