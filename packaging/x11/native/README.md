# Private native X server candidates

These source-backed providers remain **unqualified and unavailable**. There is no approved private executable checksum. `native_policy.rs` fails before a process or authority file is created. A local JSON file cannot change that trust decision. Upstream native binaries were downloaded and hashed, then examined with LLVM tools without executing them. Full input URLs, source/archive/patch checksums, native toolchain requirements and missing evidence are in `source-contract.json`; actual PE and Mach-O imports are in `upstream-dependencies.json`. The source archives remain in the private work area for offline builds. Licenses copied from the exact sources are retained in `licenses/`; their presence does not close the documented MSVC and complete XQuartz dependency redistribution gaps.

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

A native arm64 and x86_64 worker needs the Xcode macOS SDK, meson/ninja/pkg-config/bison/flex/mig, and the complete pinned XQuartz dependency source/build closure. The release `.pkg` is an inspection/dependency input; never execute its installation scripts. Configure patched xserver with `--wrap-mode=nodownload`, `-Dxquartz=true -Dxorg=false -Dxephyr=false -Dxnest=false -Dxvfb=false -Dxcsecurity=true`, an application-specific `-Dbundle-id-prefix=org.termcraft.private`, and empty Sparkle feed/key options. Use private prefix, XKB and font data paths. Build `hw/xquartz/mach-startup/X11.bin` plus the generated bundle resources. Do **not** run `meson install`: upstream's bundle install hook attempts root ownership. Copy only the build outputs, nib/resources, private xkbcomp/data/fonts and matching dependency dylibs into a fresh staging tree.

Relocate every non-system Mach-O dependency and install name to app-relative `@loader_path`/`@executable_path` paths, verify both architecture slices contain no `/opt/X11` references, and sign leaf dylibs/frameworks followed by the private app. Snapshot and pin the final signed tree including resources and code-signature files. The current `/opt/X11` release payload cannot substitute for that output. No relocated/signed native build or native build execution exists in this evidence.

The direct-entry source patch bypasses Mach registration/startx/shell startup, requires our inherited listening AF_UNIX descriptor, disables all engine-created listeners/locks, and adopts it with `ListenOnOpenFD(fd, 0)` so Xauthority remains mandatory. The stock handoff uses `noxauth=1` and is unsuitable. XKB output stays inside the private runtime; default plist lookup is app-relative and authentication overrides are volatile Objective-C `NSUserDefaults` values. The provider starts a new owned process group; macOS `waitid(WNOWAIT)` preserves its identity until owned helpers are terminated and the leader reaped. The prebound socket lives only in the private directory. Existing displays, launch services, XQuartz preference domains and shell startup files are untouched.

Composite remains explicitly disabled as in XQuartz 2.8.6's upstream known-bug policy. No extension or renderer support is inferred from strings. Existing X03 active qualification still rejects unsupported native platforms; enabling a private runtime requires native X03 implementation/evidence as well as the actual build pins.
