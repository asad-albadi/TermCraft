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

## Native arm64 candidate

The pinned xorg-server archive matched
`36db027489d6370ec5b1b6d07c8155c547193c2a6914fd617eb4311fb029257f`.
The build used Apple clang 17.0.0 with SDK 26.2, Python 3.11, Meson, Ninja,
pkg-config and the exact XQuartz distribution gitlinks. The private prefix
supplied xorgproto, Mesa DRI headers, xkbcomp, XKB data, fonts and the
recursive Mach-O dylibs. FreeType was built without optional Brotli, BZip2,
PNG and HarfBuzz dependencies. The font install set `FONTROOTDIR` explicitly.
LibX11 used `LC_ALL=C` for its locale table generator.

The staged candidate includes 818 hashed files, 12 private dylibs, the
generated bundle resources, core X fonts and 21 corresponding dependency
source archives. All candidate Mach-O load commands declare a minimum of
macOS 14.0. They were executed on macOS 26.6.2. The first probe build used
Homebrew dylibs with a macOS 26 floor and is superseded by this source build.

| Item | SHA-256 |
| --- | --- |
| Local candidate ZIP | `8809e99e1df82b7ebba8b72587c932bb2822d6089edc98dbbc839ec74078c83c` |
| Signed X11.bin | `30cff793431ef36178d9814eeb13024331c2acc28c7daf608ce575fd3a9893cd` |
| NATIVE-BUILD.json | `d628e6be7212a2708cbf2dcb94d4e5718e821dbe9216a3df9bd68e960280c1d9` |
| DEPENDENCIES.json | `356c03f349491bf660b70908eaa297636cc55f7e21fd7b45086a5c6c8d00c0cb` |

The native probe rejected anonymous and incorrect-cookie setup, accepted the
owned cookie, returned an authenticated extension list, and cleaned up the
owned process and directory. XKEYBOARD, RENDER, RANDR and GLX were advertised.
Composite was absent. The separately enabled Rust owner test verified the
same manifest digest, executed its immutable snapshot, checked pre-cancel,
verified the dedicated process group, and observed the child, authority,
socket and snapshot gone after dropping the lease.

Replay the wire checks with
`python3 packaging/x11/native/probe-macos.py CANDIDATE EVIDENCE`.
For the production gate, build the package with
`scripts/package-release.py --xquartz-runtime CANDIDATE`, relocate the app
and set `TERMCRAFT_TEST_XQUARTZ_ROOT` to its `Contents/Resources` directory.
Run `cargo +1.96.1 test --locked --lib local_candidate_uses_verified_snapshot_and_cleans_up -- --ignored --nocapture`
and `cargo +1.96.1 test --locked --test ssh_authentication managed_ssh_forwards_to_packaged_xquartz -- --ignored`.

`macos_pin()` now approves the exact private windowed arm64 inventory
`0fd1bb3ffd162962691d4b7c0b6283b53e448b6a50a51c502c86074b5317f171`.
The source-built runtime does not start or register the user's XQuartz.
The packaged runtime passed authentication, core pixel readback and private
clipboard exchange on macOS 26.6.2 and 14.8.7. Native Direct and Indirect XDMCP also passed against an owned Debian xdm.
See [XDMCP evidence](xdmcp-macos.md). Release distribution and host clipboard
interoperability remain open.

On a fresh Sonoma guest, the windowed package exposed no RandR outputs,
CRTCs, or modes. Its legacy screen-info response could not be decoded by
`x11rb`. GLX listed 32 visuals and rejected every attempted indirect context
creation with `BadValue`, consistent with the private launcher disabling
indirect GLX. The approved windowed profile provides core X11 rendering and
private X11 selection exchange. External monitor and GPU workflows are
tracked separately.
