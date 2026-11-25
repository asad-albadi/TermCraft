# macOS native artifact acceptance

The runtime gates continue to reject unqualified native artifacts. A future
artifact record must identify the exact source revision, target architecture,
deployment target, SDK/toolchain, package file hashes, Mach-O architecture
slices, relocated install names, bundled resources, and complete dependency
closure. The private XQuartz builder additionally rejects `/opt/X11` and
records `NATIVE-BUILD.json`.

Source tests and the builder contract are not a compiled-artifact qualification.
`macos_pin()` remains closed until both arm64 and x86_64 outputs have been
produced, inspected, and exercised through the authenticated X11 lifecycle.
