# Private XQuartz Build Contract

The branch now contains `packaging/x11/build-private-macos.sh` for the source-backed private engine path. It is intentionally separate from `build-macos.sh`, which only expands the pinned upstream `.pkg` for dependency inspection.

The private builder:

- requires an explicit pinned xorg-server source archive;
- applies `xquartz-private-entry.patch` through `prepare-source.py`;
- configures XQuartz with `--wrap-mode=nodownload`, `-Dxquartz=true`, `-Dxcsecurity=true`, the disabled non-XQuartz targets, and the `org.termcraft.private` bundle prefix;
- builds arm64, x86_64, or both and creates a universal image with `lipo`;
- rejects `/opt/X11` references in the staged executable/dependencies;
- relocates non-system Mach-O dependencies and ad-hoc signs staged images;
- records source/archive, SDK, architecture, and file hashes in `NATIVE-BUILD.json`.

Native qualification remains open. The pinned xorg-server archive was fetched and matched `36db027489d6370ec5b1b6d07c8155c547193c2a6914fd617eb4311fb029257f`; a native Meson configure reached Apple clang 17.0.0/arm64, then stopped because `pkg-config` and the pinned `xorgproto` subproject closure are not installed on this host. The compiled `macos_pin()` gate remains closed until the complete dependency/resource closure and the authenticated inherited-listener, process-group, cancellation, graphics, and XDMCP workflows are actually qualified.
