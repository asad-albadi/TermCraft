# Private native X server candidates

Release approval is pending for both native platforms. `native_policy.rs` rejects unapproved payloads before creating a process or authority file. Windows has exact source-built candidate inventories and private package evidence. Upstream binary hashes serve only as inspection inputs. Full input URLs, source/archive/patch checksums, native toolchain requirements and remaining approval evidence are in `source-contract.json`. PE and Mach-O imports for the upstream binaries are in `upstream-dependencies.json`. The source archives remain in the private work area for offline builds, and exact-source license files are retained in `licenses/`. [Windows native evidence](../../../docs/parity-evidence/execution/2026-11-25-windows-native.md)

Prepare either source tree with the pinned **local** source archive:

```
python3 packaging/x11/native/prepare-source.py --platform windows --source-archive /offline/vcxsrv-source.tar.gz --destination /unused/private-vcxsrv-source
python3 packaging/x11/native/prepare-source.py --platform macos --source-archive /offline/XQuartz-xserver-source.tar.gz --destination /unused/private-xquartz-source
```

This validates the archive, safely extracts it, applies the exact private patch, retains corresponding source and records every prepared regular-file checksum. It does not download, install, execute an upstream installer, change a daemon or modify an existing display.

## Windows native build recipe

Run `build-private-windows.ps1` from a native x64 Visual Studio 2022 Developer
PowerShell. Provide the pinned source archive, an unused build directory
without spaces, and the native tools. The opt-in `windows_x_fixture` release
workflow records the dependency versions and verifies all downloaded hashes.

```powershell
$tools = @('C:/tools/jom', 'C:/tools/nasm', 'C:/tools/perl/perl/bin',
    'C:/tools/winflexbison', 'C:/tools/gperf/usr/bin')
./packaging/x11/native/build-private-windows.ps1 `
    -SourceArchive C:/offline/vcxsrv-source.tar.gz `
    -BuildDirectory C:/tc-x11-build `
    -PythonExecutable C:/tools/x11-python/Scripts/python.exe `
    -GitUnixBin 'C:/Program Files/Git/usr/bin' -ToolDirectories $tools
```

The Python environment needs lxml, Mako, PyYAML, Meson and Ninja. The recipe
adapts the pinned upstream tool paths in its prepared copy, builds the native
components and stages the adjacent DLLs, fonts, xkbcomp and XKB data. Its
output includes `build-inputs.json`, `candidate-inventory.json` and
`pe-imports.txt`. The output stays in the build directory.

For an existing build, `stage-private-windows.ps1` takes the prepared source
directory, an empty candidate destination and the x64 VC143 CRT directory.
Save its JSON output outside the candidate directory. The qualification test
expects `x11/windows-x86_64-private1` beneath `TERMCRAFT_TEST_WINDOWS_X11_ROOT`
and the JSON report at `TERMCRAFT_TEST_WINDOWS_X11_INVENTORY`.

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
