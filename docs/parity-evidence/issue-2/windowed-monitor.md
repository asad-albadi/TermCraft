# Private macOS XQuartz windowed geometry

Audit baseline: TermCraft `6cb3b87`, Apple Silicon macOS 26.6.2 (25G83), exact
private XQuartz arm64 inventory. All 818 hashes passed, including the
`30cff793…` X11.bin. The startup diagnostics use the existing package and runtime policy.

## Observed on two fresh private displays

- Core `GetGeometry(root)` and X11 setup each reported 1920×1050.
  `XINERAMA IsActive` was true and `QueryScreens` returned the rectangle
  `(0, 0, 1920, 1050)`. These are XQuartz logical coordinates, not
  physical panel pixels, a macOS display identifier, or proof of Retina
  behavior. The corresponding source normalizes the rectangles to the
  root union and may leave space for the macOS menu bar. Its headless
  fallback can report a stub rectangle.
- RandR reported 1.5. On both launches, `GetScreenResourcesCurrent` was
  empty before other RandR resource queries. On one fresh server,
  `GetMonitors(active=true)` returned one automatic rectangle and caused
  the cached list to become 1 CRTC, 1 output and 3 modes. On another,
  non-cached `GetScreenResources` did the same. The output was named
  `default` in both cases. The two 1920×1080 entries and the 1920×1050
  entry are compatibility modes, not qualified physical outputs or
  safe choices. In the corresponding source, `RRGetInfo` calls
  `RRScanOldConfig` to synthesize `default`; `GetMonitors` can trigger
  that path. The cold cache alone establishes neither the absence of useful
  geometry nor that resources stay empty after later client queries.
- Mapping and resizing an owned ordinary X window from 80×64 to 123×87
  yielded matching core `ConfigureNotify` and `GetGeometry(window)`.
  The root and Xinerama data were unchanged, and no root RandR
  screen-change event accompanied that window resize. This verifies an
  X client window update, not AppKit drag/placement, root reconfiguration,
  hotplug, or live topology update support.

## Implemented startup scope

1. The macOS probe reads core `GetGeometry(root)` and optional Xinerama
   `IsActive` / `QueryScreens` on the authenticated display. Rectangles
   preserve signed positions in the macOS-only capability field.
   Unavailable layout data leaves any valid root geometry intact.
   RandR is reported by version only. The probe neither reads the
   resource cache nor materializes it with a forced query.
2. The Services X11 card labels observations as startup-only and in
   logical X11 coordinates. RandR is advertised, not verified for mode
   changes. The UI offers no monitor, full-screen or resolution control.
   The XDMCP controls and launch validation are unchanged, as are the
   Linux mode/restore probe and UI.

## Separate qualification for live updates

The pinned source routes Xplugin `XP_EVENT_DISPLAY_CHANGED` through
`kXquartzDisplayChanged` to `QuartzUpdateScreens`, then refreshes the
legacy RandR state. `QuartzUpdateScreens` rebuilds PseudoramiX,
resizes the root, repositions windows and sends root
`ConfigureNotify`. `RRScreenSizeNotify` can short-circuit if root
dimensions and mm values are unchanged, so RandR screen-change alone
is not a topology-change contract. Confirm live monitor changes with a physical display while observing a
fresh root and Xinerama snapshot. Ordinary window resize does not imply
that the host topology changed.
The earlier Sonoma legacy decode observation was not retested here.

The native test covers the authenticated startup snapshot and owned window
resize. Unit tests cover empty and invalid layouts, signed rectangles,
the UI with and without a layout, and unchanged option rejection. The
native run with a fresh TermCraft executable on the 26.6.2 host returned
1920×1050 and the logical rectangle `(0, 0, 1920, 1050)`. Its owner
test also passed the window resize and event checks and verified teardown.
There are no RandR setters, display capture, selectable modes, source
rebuild/re-pin, or new controls. “Unsupported” here is TermCraft policy,
not a guarantee that arbitrary X clients cannot request a server mode
change. Corresponding source still wires the legacy mode callback to
CoreGraphics capture/mode code under its own conditions. A server-side
enforcement change would be a separate qualification and repin. See [direct GLX evidence](xquartz-build.md#direct-apple-glx-on-apple-silicon).

## Reproduction

With an explicitly staged pinned package and a matching TermCraft build
in its adjacent `Contents/MacOS` directory, run from the worktree:

```sh
cargo +1.96.1 test --locked --lib x11::native_macos::tests
cargo +1.96.1 test --locked --bin harbor-workspace macos_xserver_snapshot
TERMCRAFT_TEST_XQUARTZ_ROOT=/path/to/TermCraft.app/Contents/Resources \
  cargo +1.96.1 test --locked --lib local_candidate_uses_verified_snapshot_and_cleans_up -- --ignored --nocapture
```
