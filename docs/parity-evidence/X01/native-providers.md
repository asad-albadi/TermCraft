# X01 private native provider engineering — 2026-11-25

**Status: partial source implementation, native runtime and redistribution qualification unavailable.** This is not X01 completion and changes no checklist count. Existing Linux pinned/owned Xwayland is preserved. Native private output checksums are absent by design, so a compiled approval gate rejects native starts before creating authority or launching a process. Neither a folder-local manifest nor PATH can enable stock binaries.

Implemented privately:

- Windows native suspended process creation, kill-on-close job, immutable-to-writers snapshot PE/DLL handles, protected owner-only directory DACL, one reserved loopback port, fixed display, exact listener PID proof through IP Helper, anonymous rejection and cookie-authenticated protocol readiness, cancellation and X02 generation lease fields.
- macOS direct executable launch from a verified private snapshot, prebound private Unix listener passed as the only intentional listening FD, owned process group with waitid(WNOWAIT) identity-safe helper cleanup, private authority, bounded anonymous/authenticated protocol readiness and X02 environment/owned-only socket forwarding.
- Actual pinned source changes: Windows loopback-only/exclusive listener and no scan; XQuartz direct no-service entry, authenticated inherited listener, no global lock/listener path, private XKB output, app-relative defaults, volatile Objective-C security defaults, no automatic clipboard proxy. Native Rust components compile against real Windows/macOS APIs; upstream source patches apply to pinned source archives.
- Offline source preparer verifies source archives, extracts with traversal protection, applies patches, retains corresponding source and hashes the prepared tree. Native source/license/dependency contracts and extracted-source licenses are included. Verified input PE and both Mach-O slice imports are recorded.

Primary provenance: [VcXsrv 21.1.16.1](https://github.com/marchaesen/vcxsrv/releases/tag/21.1.16.1), [VcXsrv exact source](https://github.com/marchaesen/vcxsrv/tree/b1c7712a2584b263f44fa24e8c493f4f59c472ea), [XQuartz 2.8.6 release and component source pins](https://www.xquartz.org/releases/XQuartz-2.8.6.html), [XQuartz xserver exact source](https://github.com/XQuartz/xorg-server/tree/0f7f5eb17c2e22528ecd2cf7feff11da85c7dcbc). Exact downloaded hashes are in packaging/x11/native/source-contract.json; no upstream payload was executed here.

Verified command results in the private work area's evidence directory:

- `native-red.log` and `transport-red.log`: missing API RED before portable implementation.
- `ssh-owned-red.log`: owned-only socket parser missing before implementation.
- `native-green.log`: seven portable fixture tests passed, covering immutable snapshot bytes after source mutation, bad hash/path/symlink rejection, exclusive authority creation, exact cookie transmission, private Unix transport, and cancellation before connection.
- `ssh-owned-green.log`: six embedded SSH X11 tests passed, including owned-only absolute socket handling with System source behavior preserved.
- `full-lib.log`: 102 library tests passed, 5 intentionally ignored, zero failures. This is the private library suite, not a whole-app integration-suite claim.
- `windows-component.log`: real provider/image/job/IP Helper APIs checked for x86_64-pc-windows-msvc.
- `macos-component.log`: real provider/image/fd/process-group/waitid APIs checked for aarch64-apple-darwin.
- `source-stage-red.log`/`source-stage-green.log`: two offline-source behavior tests passed after absent preparer RED. Both exact pinned source archives were also prepared and patched successfully (windows-source-prepare.log, macos-source-prepare.log).

Not verified: native source C/Objective-C build/link, native process/job/group cleanup, native actual authority ACL/access controls, port/display collision behavior, signed snapshot loader/dependency closure, native windows/test client visibility, software/hardware graphics, monitor/fullscreen behavior, complete native redistribution. Cross-checking Rust is not native runtime evidence. XQuartz upstream explicitly disables Composite due known bug 1409; the private patch preserves that limit. X03 still refuses unsupported native graphics probes. A native private build, per-file compiled checksum approval, license/source/dependency closure and actual native lifecycle/X03 tests are required before enabling either provider. No tests used host X sockets/displays or ran native payloads; protocol fixtures owned their private socket/loopback listeners.

Root integration must preserve typed X04 provider modes and capabilities. These private APIs currently expose only default private/authenticated local transport. XDMCP and clipboard remain explicit future option/capability branches; no public TCP access was added to satisfy them.
