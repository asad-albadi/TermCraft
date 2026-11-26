# Private native X server candidates

These source-backed providers remain **unqualified and unavailable**. There is no approved private executable checksum. `native_policy.rs` fails before a process or authority file is created. A local JSON file cannot change that trust decision. Upstream native binaries were downloaded and hashed, then examined with LLVM tools without executing them. Full input URLs, source/archive/patch checksums, native toolchain requirements and missing evidence are in `source-contract.json`; actual PE and Mach-O imports are in `upstream-dependencies.json`. The source archives remain in the private work area for offline builds. Licenses copied from the exact sources are retained in `licenses/`; their presence does not close the documented MSVC and complete XQuartz dependency redistribution gaps.

Prepare either source tree with the pinned **local** source archive:

```
python3 packaging/x11/native/prepare-source.py --platform windows --source-archive /offline/vcxsrv-source.tar.gz --destination /unused/private-vcxsrv-source
python3 packaging/x11/native/prepare-source.py --platform macos --source-archive /offline/XQuartz-xserver-source.tar.gz --destination /unused/private-xquartz-source
```

This validates the archive, safely extracts it, applies the exact private patch, retains corresponding source and records every prepared regular-file checksum. It does not download, install, execute an upstream installer, change a daemon or modify an existing display.

## Windows native build recipe

Use a Windows x86_64 native worker with Visual Studio 2022/VC143 and the Windows SDK. The pinned source's `buildall.sh` includes host-specific tool paths and an upstream NSIS installer step. On a native x64 Developer PowerShell, the underlying release build can run directly in the prepared source tree:

```powershell
$env:IS64 = '1'
$env:CFLAGS = '-FS'
$env:CL = '/FS'
$env:MHMAKECONF = (Get-Location).Path
$env:PYTHON3 = (Get-Command python.exe).Source
msbuild freetype/MSBuild.sln -t:Build -p:Configuration=Release -p:Platform=x64 -m:2
New-Item -ItemType Directory -Path openssl/release64
Push-Location openssl/release64
perl.exe ../Configure VC-WIN64A --release
jom.exe /J 4
Pop-Location
Push-Location pthreads
nmake.exe VC-static EXTRAVERSION=
Pop-Location
msbuild tools/mhmake/mhmakevc10.sln -t:Build -p:Configuration=Release -p:Platform=x64 -m:2
tools/mhmake/Release64/mhmake.exe -P4 -C xorg-server MAKESERVER=1
```

Make NASM, Jom, Strawberry Perl, WinFlexBison, Python with lxml, Mako, PyYAML, Meson and Ninja, and GNU gperf available in this shell. Resolve the pinned tree's hard-coded WinFlexBison and Python launcher paths through that shell's PATH, and use Git Bash for the two upstream `wsl ./` recipes in `xorg-server/dix/makefile` and `xorg-server/xkeyboard-config/makefile`. Keep MSVC's executable directories ahead of Git's Unix tools so the build uses the MSVC linker. Preserve the native backslash form of `MHMAKECONF` because mhmake compares directory prefixes literally. Use a build directory with no spaces.

Stage the server, `xorg-server/obj64/servrelease/vcxsrv.exe`, with the matching DLLs, fonts, xkbcomp and XKB data from the same build. The upstream x64 NSIS recipe lists the runtime files. Record exact PE imports, file hashes, toolchain versions, compiler options, corresponding source, the TermCraft patch and the MSVC redistribution notices before approving a payload.

Use `stage-private-windows.ps1` with the prepared source directory, an empty candidate destination and the x64 VC143 CRT directory. Save its JSON output outside the candidate directory. The qualification test expects the staged directory at `x11/windows-x86_64-private1` beneath `TERMCRAFT_TEST_WINDOWS_X11_ROOT` and the JSON report at `TERMCRAFT_TEST_WINDOWS_X11_INVENTORY`.

To exercise the private Windows capability probe with the authenticated
wire fixture, pass a native TermCraft executable as
`tests/windows_xdmcp_candidate.py --capability-probe`. The JSON report records
the executable digest, live extension versions and actual Composite pixels.
The probe leaves the screen mode and indirect GLX policy unchanged.

The transport patch changes listener binds to IPv4/IPv6 loopback, configures checked SO_EXCLUSIVEADDRUSE on Windows, and rejects a missing explicit display or displayfd scanning. The Windows SDK expands SO_EXCLUSIVEADDRUSE through SO_REUSEADDR, so the patch keeps the header's constant defined and excludes its reuse branch on Windows. The Rust provider reserves exactly one OS-selected loopback port, creates a suspended process with no inherited handles, assigns it to a kill-on-close job, releases the reservation, resumes, checks IP Helper ownership of the exact listener and requires anonymous rejection plus cookie-authenticated setup. A collision fails without scanning or altering the competing server. Snapshot PE/DLL handles deny write/delete sharing for the entire lease. The old upstream checksum is never an approval for this changed output.

Windows XKB starts the adjacent `xkbcomp.exe` directly with explicit input and output paths. It inherits the server's job and does not load Command Processor startup hooks. The server reads its own runtime directory for user-level X configuration.

The private Windows XDMCP provider accepts one explicit local IPv4 address
with per-session consent. It reserves the chosen address and display port,
passes the restriction to the private engine, and verifies that the resulting
listener belongs to the owned process. The regular owned display continues
to reserve loopback. Both paths require protocol authentication, including
binary Xauthority cookies and credentials negotiated by an XDMCP manager.

## macOS native build recipe

A native arm64 and x86_64 worker needs the Xcode macOS SDK, meson/ninja/pkg-config/bison/flex/mig, and the complete pinned XQuartz dependency source/build closure. The release `.pkg` is an inspection/dependency input; never execute its installation scripts. Configure patched xserver with `--wrap-mode=nodownload`, `-Dxquartz=true -Dxorg=false -Dxephyr=false -Dxnest=false -Dxvfb=false -Dxcsecurity=true`, an application-specific `-Dbundle-id-prefix=org.termcraft.private`, and empty Sparkle feed/key options. Use private prefix, XKB and font data paths. Build `hw/xquartz/mach-startup/X11.bin` plus the generated bundle resources. Do **not** run `meson install`: upstream's bundle install hook attempts root ownership. Copy only the build outputs, nib/resources, private xkbcomp/data/fonts and matching dependency dylibs into a fresh staging tree.

Relocate every non-system Mach-O dependency and install name to app-relative `@loader_path`/`@executable_path` paths, verify both architecture slices contain no `/opt/X11` references, and sign leaf dylibs/frameworks followed by the private app. Snapshot and pin the final signed tree including resources and code-signature files. The current `/opt/X11` release payload cannot substitute for that output. No relocated/signed native build or native build execution exists in this evidence.

The direct-entry source patch bypasses Mach registration/startx/shell startup, requires our inherited listening AF_UNIX descriptor, disables all engine-created listeners/locks, and adopts it with `ListenOnOpenFD(fd, 0)` so Xauthority remains mandatory. The stock handoff uses `noxauth=1` and is unsuitable. XKB output stays inside the private runtime; default plist lookup is app-relative and authentication overrides are volatile Objective-C `NSUserDefaults` values. The provider starts a new owned process group; macOS `waitid(WNOWAIT)` preserves its identity until owned helpers are terminated and the leader reaped. The prebound socket lives only in the private directory. Existing displays, launch services, XQuartz preference domains and shell startup files are untouched.

Composite remains explicitly disabled as in XQuartz 2.8.6's upstream known-bug policy. No extension or renderer support is inferred from strings. Existing X03 active qualification still rejects unsupported native platforms; enabling a private runtime requires native X03 implementation/evidence as well as the actual build pins.
