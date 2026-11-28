# RDP window movement and scrolling

The owner reports that the previous latency review connects and works okay, with remaining lag when moving or scrolling windows compared with Termix. This follow-up addresses a measured GUI bottleneck and enables additional graphics capabilities. Actual Windows smoothness still requires the owner's check. PR #8 remains draft and unmerged.

## Changes and comparison

Read-only research used [Termix remote desktop documentation](https://termix.site/plugins/remote-desktop/) and its [native RDP source](https://github.com/Termix-SSH/Plugin-Remote-Desktop/blob/366813b202778b46ee936b361ac603f787bcac9e/native/engine/src/rdp.rs), pinned at `366813b202778b46ee936b361ac603f787bcac9e`. Its IronRDP setup enables dynamic graphics and the official default bitmap codec capability set. TermCraft previously advertised neither. This comparison establishes a capability difference, not proof of the codec or settings used in the owner's Termix session. Implementation uses pinned IronRDP APIs; no Termix implementation was copied.

- Register a reopenable graphics channel and advertise V8 small-cache capabilities, without AVC/H.264. The pinned library decodes/composites supported graphics commands and acknowledges frames. Graphics resets publish a complete baseline and update the UI's extent when it changes.
- Enable official default classic RemoteFX capabilities, allowing enhanced surface updates and frame acknowledgment as supported by the server. Classic bitmap fallback stays available. TLS/NLA and certificate checks remain intact.
- Convert remote pixels in the optimized GUI crate, removing the cached development dependency's slow full-image conversion from this rendering path. RDP frames use the helper's guaranteed opaque alpha; VNC and cursor images retain ordinary alpha conversion. Existing dirty-region uploads and bounded output remain in use.
- Build the current GUI code at optimization level 2 with development assertions enabled so adjacent review helper discovery and review provider selection remain correct. The helper remains a release build. Cached GUI dependencies are still development dependencies; this is a local review bundle, not a qualified production release.

RemoteFX quantizes colors: the pinned fixture's red 255 decodes to 254. The endpoint test explicitly checks this sentinel to prove RemoteFX was selected; its narrow color tolerance does not weaken the exact-color bitmap tests or promise a universal one-level error bound. The existing `lossy_compression: false` setting is retained but does not make RemoteFX mathematically lossless. There is no H.264 or hardware decoder in this change.

The pinned graphics compositor has a separate 256 MiB cache bound. TermCraft still validates the published desktop against its 16-million-pixel limit. That validation follows library compositing; it does not constrain every intermediate graphics allocation to the desktop byte limit.

## Measured conversion cost

Ten conversions per size on this machine, excluding network delivery, decoding and GPU upload:

| Image | Cached dependency conversion | Current production opaque conversion |
| --- | ---: | ---: |
| 1920 × 1080 | 279.349 ms/frame | 2.769 ms/frame |
| 3840 × 2160 | 1064.240 ms/frame | 10.280 ms/frame |

The alpha-aware production path measured 3.388 and 14.668 ms/frame respectively. These are CPU microbenchmarks, not end-to-end FPS or a comparison benchmark against Termix. The source module was compiled with optimization level 2 and development assertions, matching the current GUI crate's settings.

## Validation and acceptance

- [x] 106 helper checks passed with one test thread, including graphics channel reopening, compressed Planar decoding at a nonzero offset, exact dirty packet bytes and frame acknowledgments. One pre-existing serial-device timing test failed in the initial parallel run; the full serial run passed.
- [x] Nine independent endpoint/bundle checks passed, including authenticated RemoteFX decoding and ordered key transitions, exact bitmap rendering, adjacent helper selection, certificates, closure, pane resize, input ordering during resize and safe failed-NLA diagnostics.
- [x] Fifteen UI checks passed, including actual typed/pasted authentication, partial texture updates, full-before-patch invalidation, alpha conversion, pane sizing, cursor/font and existing FTP ownership regressions.
- [x] Native Wayland startup/screenshot, formatting and approved session-dialog scope checks passed.
- [x] All 243 pinned Chromium files retained and hash-verified with notices/licenses.
- [ ] Owner checks movement and scrolling on the actual Windows endpoint; protocol-level graphics tests do not replace this check.
- [ ] Exact-head hosted CI and full owner human acceptance, followed by explicit merge approval.

Current components and source hashes are in [the graphics review manifest](rdp-graphics-review-build.json). Earlier evidence and builds remain historical and intact. Close the older application completely before running the stable launcher, since an existing process may receive the launch request:

```bash
"/home/asad/.codex/worktrees/termcraft-issue-3/app/target/issue-4/run-terminal-row-review"
```
