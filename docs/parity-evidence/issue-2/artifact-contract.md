# macOS native artifact acceptance

The runtime gates continue to reject unqualified native artifacts. A future
artifact record must identify the exact source revision, target architecture,
deployment target, SDK/toolchain, package file hashes, Mach-O architecture
slices, relocated install names, bundled resources, and complete dependency
closure. The private XQuartz builder additionally rejects `/opt/X11` and
records `NATIVE-BUILD.json`.

`macos_pin()` approves the exact arm64 inventory for authenticated private
windowed X11. Package staging and runtime startup both check hashes before
using the staged tree. Distribution, the macOS 14 runtime replay, active
graphics and XDMCP have open evidence requirements.
