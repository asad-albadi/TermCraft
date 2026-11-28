# Human-review correction — 2026-11-27

**Latest owner-directed pass:** [all remaining panes, the single combined bar, the shared SFTP sidebar and icons](all-pane-cleanup.md), source `05a2234`. The approved session create/edit dialogs remain unchanged. [Current native previews](captures/unified-workspace/README.md). All three source-platform CI jobs, all three unsigned package jobs and aggregate verification passed. [Verified build identities](all-pane-builds.json) and the [current Linux command](all-pane-cleanup.md) are available. Historical entries below retain their own source/build identities. Owner human checks and explicit merge approval remain open.


## Current pane refinement — ready for owner review

The owner approved the session create/edit dialogs and identified remaining clutter in the connected remote-files pane. Those dialogs are unchanged. The file pane now has a compact path/navigation row and Upload/Download/More strip, with shell integration, transfer preferences and guidance available on demand. Its file list takes the remaining height, with a small Downloads footer and a reserved right-edge scrollbar gutter. A narrow neighboring terminal moves Find/Reconnect into More to keep its status clear.

- [x] Preserve the approved session create/edit dialogs.
- [x] Prioritize file rows; remove permanent setup/instruction blocks and duplicate path/transport rows.
- [x] Keep SFTP/SCP identity explicit; keep overwrite-on state, active transfer progress, capability gates and confirmations.
- [x] Verify row-menu targets, path validation, file-drop boundaries, short-window geometry and stable idle panel width.
- [x] Pass all 21 focused cases and current Linux, Windows x64 and Apple Silicon native CI.
- [x] Pass all three native core package startup/lifecycle jobs and verify downloaded payload identities and hashes.
- [x] Update the local Linux launcher to the verified pane-refinement package, preserving earlier runtimes.
- [ ] Owner approves the refined panes and explicitly authorizes merging PR #8.

Close the older application copy before running:

```bash
"/home/asad/.codex/worktrees/termcraft-issue-3/app/target/issue-4/run-latest-linux"
```

The app package identifies UI source `db07f66`. [Current native CI](https://github.com/asad-albadi/TermCraft/actions/runs/46059467626) passed all three platforms at `8f121b2`, which changes only one `#[cfg(test)]` fixture body: it now targets the Downloads footer and an actual file-list row instead of the removed instruction labels. Both original real-SSH assertions remain: a native drop on Downloads must not upload private data, while a drop on the file list must upload the exact bytes. All tracked repository bytes outside that test body are identical between those commits.

[Build identities and qualification limits](pane-builds.json) keep package source and amended test source separate. The [Linux package job](https://github.com/asad-albadi/TermCraft/actions/runs/46057357806/job/140703065648), [Mac package job](https://github.com/asad-albadi/TermCraft/actions/runs/46057357806/job/140703065598) and [Windows package job](https://github.com/asad-albadi/TermCraft/actions/runs/46057357806/job/140703065568) all succeeded. The earlier workflow's Linux/Mac regression lanes failed the obsolete selectors, so its aggregate and publication were skipped. That workflow is **not** described as successful. Its valid UI packages are reused after the test-only correction, with current three-platform CI as separate evidence. This is unsigned core qualification for human UI review, not a new release assembly or optional-provider bundle.

Linux archive SHA-256: `2ba3b60facec31a9c1166bb46dd96241020d3c83a123beee9fb6fffd82868d37`. Executable SHA-256: `cd4fc7e1c342692ee7320ea170692971457681ad82a8787478fd79c4a57049bb`. Four distribution hashes, 643 exact Git source inputs, 1,097 extracted files and matching archive-bound X11/Wayland/lifecycle evidence were verified after download. Mac and Windows distribution hashes and source IDs were also verified.

[Seven native development previews](captures/pane-refinement/manifest.json) cover Light/Dark, Full HD, short and portrait windows and the More menu. These render the actual UI with an explicitly synthetic listing using cached pre-adapter dependencies; they are not live-SFTP, packaged-rendering, physical hardware or owner-acceptance evidence. Earlier screenshots retain their original provenance.

PR #8 remains draft, open and unmerged. Owner visual, physical-platform and assistive-technology checks remain open. No merge, release or tag was made.


The owner rejected the prior c2 UI as cluttered/inconsistent. Prior passing tests remain historical technical evidence, NOT human acceptance. PR8 draft; NEVER merge until explicit human approval. Scope is authorized continuation, no new permission milestones.

## Reference and correction design
Actual MobaXterm session screenshot inspected by controller: target/issue-4/references/mobaxterm-session.png (NeSI https://docs.nesi.org.nz/Getting_Started/Accessing_the_HPCs/MobaXterm_Setup_Windows/). Protocol icons, compact host/username/port basics, advanced/terminal/network/bookmark tabs. Official https://mobaxterm.mobatek.net/documentation.html corroborates fields. Do not copy outdated Windows visual style: preserve Windows11 Fluent styling, green logo accent.

Six owner screenshots under /tmp/codex-clipboard-{73ce984b-e9e5-43a2-a61f-5b7d169dbfd6,2b5fb081-295f-4e78-8e8b-1bdab1875c45,68603fef-91db-4f37-ab46-dbe9f5c3c891,b3a7aad2-2b49-4887-bc5e-c818e2a75b62,f27375d0-00e9-4e92-b074-ce6b0952da5c,87044118-55d8-4015-a4ef-a7012c4d8c75}.png.

C1 Session creation: default simple essentials first, protocol chooser with existing icons; advanced/terminal/network/bookmark or appropriate protocol tabs. No name/folder or dozens of advanced options crowding default page. All existing options remain reachable, values preserved across tabs, fixed Connect/Save only/Cancel footer. SSH and RDP defaults should fit without body scroll at ordinary desktop dimensions. RDP security/display/clipboard/redirection organized, prose replaced by accessible help on demand. No auth/default/storage/network changes. Safety warnings remain at relevant decision.
C2 Main workspace: terminal gets one compact action row; move always-visible profile/highlight/split-layout/history diagnostics into appropriate More/settings popup. MultiExec only when activated, retain explicit targets/safety. Preserve pane identity/focus with less decoration for sole pane. Sidebar quickconnect/search/tree, one empty state, shared sources manager and keyboard instructions in overflow/help rather than permanent text blocks.
C3 Page layout: vertical scrollbar at right edge of its own full-width viewport, dedicated gutter with no content overlap. Readable content cap INSIDE full-width viewport, not enclosing ScrollArea natural-width shrink. Settings category navigation/equal-width organization and concise help. Games fills whole client background, responsive launcher grid rather than narrow 13-button column. Detached window compact reattach/overflow actions, native title for identity, history hidden by default. No changes to game logic, dirty guards, terminal renderer, identifiers, trust, credential storage.


## Previous platform checks (df5dbb8)

Source `df5dbb8` passed [native CI on all three platforms](https://github.com/asad-albadi/TermCraft/actions/runs/46049020536). The [unsigned candidate workflow](https://github.com/asad-albadi/TermCraft/actions/runs/46049050604) separately passed all three regression lanes and all three native package jobs. The downloaded Linux, Windows x64 and Apple Silicon macOS distribution files match their candidate source IDs and SHA-256 inventories; [human-builds.json](human-builds.json) records those exact identities. The [combined inventory check](https://github.com/asad-albadi/TermCraft/actions/runs/46049050604/job/140691982468) passed, and publication was skipped. These are unsigned core builds, not new production optional-provider bundles.

The first standalone Mac attempt timed out at the original 20-second asynchronous vault-unlock wait, before the added nested-dialog assertions. Both the separate package-workflow regression and the isolated failed-job rerun passed on unchanged source, including that case. The failed attempt remains recorded; no timeout was widened and no security assertion was removed.

No release or tag was created. PR #8 remains draft, open and unmerged. Owner visual, physical-device and assistive-technology acceptance is still outstanding.

## Previous qualified Linux build (df5dbb8)

The final Linux candidate is source `df5dbb8e260e84aca2e25d7ae19c112233cab97f`, produced by [package job 140678056174](https://github.com/asad-albadi/TermCraft/actions/runs/46049050604/job/140678056174). Core startup on X11 and native Wayland, Debian/per-user lifecycle, the AppImage and corresponding-source checks passed. All four distribution hashes, 643 Git source inputs and 1,097 extracted package files were independently verified after downloading [artifact 15225052570](https://github.com/asad-albadi/TermCraft/actions/runs/46049050604/artifacts/15225052570).

- Core archive SHA-256: `b54927c430c2afb602f8215051511894419010a9870b766c79a81cda581e932d`.
- Executable SHA-256: `ab2b02c1628a565bb0b1235197a06c2710527777a8210e8a42b366850891be46`.
- AppImage SHA-256: `755edc4ff76b7195c68a1f787ebf06dd4f1cb175cefec82dae372616937b4703`.

This previous archive remains preserved. The launcher at the top of this document now points to the newer pane-refinement package, not this earlier binary. Both use the native archive to avoid the separately documented host-specific AppImage Wayland compatibility problem.

The fourteen current gallery/correction images retain their actual `f599000` core identity. That source includes the same final UI; the Linux candidate above additionally contains the packaging inventory and test portability repairs. The screenshots have not been relabeled as captures of another binary.

## First correction status (historical)

- [x] Simplify session creation and contextual help.
- [x] Compact terminal, sidebar and detached controls.
- [x] Correct full viewport scrolling, settings and games layouts.
- [x] Inspect real native previews at normal, ultrawide and compact sizes.
- [x] Pass focused regression checks and independent review.
- [x] Produce updated human-check builds for Linux, Windows x64 and Apple Silicon macOS.
- [ ] Owner completes human checks; merging remains prohibited.

## Historical native development previews

Correction source: `72db3f4078af03cca728a1853387165875a20dc2`. [Eight original captures and source/binary identity](captures/human-correction/manifest.json) record the actual running development UI at Full HD, ultrawide, compact and detached sizes. These use a source-verified cached library with prior native-adapter dependencies. They are not packaged-build, physical hardware, screen-reader or owner acceptance evidence. The prior c2 packages do not contain these corrections.

The controller directly inspected the simple SSH/RDP pages, full-width scroll viewport, categorized settings, compact terminal, detached controls and centered games catalog. During that inspection, the first games layout and clipped compact RDP port were returned and corrected. Independent review approved the correction and the final legacy-protocol guidance fix. The source `72db3f4` passed 74 focused cases; the six-line wording correction `8c50a01` passed seven covering cases. These case sets overlap. At that checkpoint, current-source hosted CI and updated distribution packages were pending; see the latest qualification above.

![Simple RDP form at 640×480](captures/human-correction/session-rdp-dark-compact.png)

## Earlier native development preview

Source `8c50a010f8aeb96cfc03ff2e5d6fc672b40241ff` incorporates the independent review's final wording correction. The local native development executable is `target/issue-4/specialist-probe/termcraft-human-review-ready`, SHA-256 `79e0988509923ef4809e0cc7e82d054077fd6174be451d8bc732289c907363bb`. This is a UI review preview with the development dependency limits above; it is not a replacement for current-source platform CI or distribution qualification. PR #8 remains draft and unmerged. Owner acceptance is still open.

## Full CI read-only-state correction

The first full Linux run at `e8347a2` found one missed UI case: the simplified shared-session heading no longer explicitly said read-only. That application target reported 274 passed, one failed and eight ignored. The CI/package runs at that head were cancelled and are not accepted replacement builds.

Source `6e58b3c397313eb8c1aba49a8b0493f733ca1652` restores the concise “Shared sessions · read-only” heading. The test checks read-only, source name, cached/stale state and the shared session's name. The missing state was reproduced before the fix; nine covering cases passed after it, including shared-source cache/restart/no-overwrite and canceled credential/prompt guards. This set overlaps earlier focused checks. Current-source full CI and unsigned packages are being rerun after scoped review; owner acceptance remains open.

## Scheduler fixture synchronization

The corrected application's full native CI at `9c6f10c` passed on Linux, Windows x64 and Apple Silicon: [run 46037789416](https://github.com/asad-albadi/TermCraft/actions/runs/46037789416). The duplicate Linux regression in the package workflow separately hit a pre-existing scheduler-test race; all 275 application UI cases passed there too.

Test-only commit `abd621e` waits for the scheduler's published job status as well as the four child-process markers before asserting exactly two process-limit skips. It preserves the original one-second service lifetime, stopped-state, file-content and log-bound checks. Production scheduler and application code are unchanged. The updated fixture passed 30 scoped runs; the baseline also passed 30 locally, so the package CI log remains the observed failure. Scoped independent review approved the correction. Fresh full CI verifies the test-only commit.

The unsigned native packages being built at `9c6f10c` contain every production/UI correction. They remain identified by that exact source revision; a subsequent test-only change does not relabel their identity. No failed aggregate run is described as green, and no release or merge is authorized.

## Final scrollbar feedback and package repair

Native inspection of the core preview found no visible thumb-hover change. Correction `106bce9` restores an animated 2-DIP resting thumb and 8-DIP hovered/held thumb while permanently reserving the complete 14-DIP gutter. The fully expanded thumb remains six DIPs away from content. Actual paint/input tests and 12 native development captures verified both desktop widths with normal and reduced motion. Eleven focused component cases passed; scoped independent review approved the correction. These development frames do not replace final distribution qualification. The earlier core preview and its recorded screenshots remain identified by their original source and do not contain this last hover adjustment.

The AppImage builder's obsolete Alpine dependency pin is repaired at `c601692`: the exact pin now matches the live official `libffi=3.8.2-r0` provider. The original failure was reproduced against the pinned base image; the updated full builder and offline exact-version checks passed. All six AppImage contract methods passed, with the finished-artifact class correctly skipped until an AppImage and source sidecar exist. Scoped independent review approved the pin update. Fresh full CI and unsigned packages will verify the final source together; no release or merge is authorized.

The earlier Linux core contained the decluttered forms and screens and passed a separate startup screenshot on the owner's Wayland host without an environment workaround. The launcher now targets the qualified Linux candidate above, including the final scrollbar feedback adjustment and packaging repairs. The earlier host screenshot remains evidence for its original binary only.

## Complete AppImage source-inventory repair

The builder pin repair also required refreshing the recorded package-list checksum. A full offline runtime rebuild verified all 24 recorded files; only the expected package-list digest differed. The runtime executable, relink objects, compiler records and notices retained their existing digests. The record now includes the builder-pin file digest, with a new fast contract test that fails when a pin edit omits its inventory update.

A complete local AppImage and matching source archive were built and verified using the earlier 9c6f10c core as a format fixture. All eleven finished-image/contract tests passed with zero skips. This is a local format proof, not a relabeled current-source candidate. Current application source f599000 passed all three native CI jobs; final hosted package qualification is rerun after this metadata repair. No merge or release is authorized.

The added pin/inventory guard now compares canonical LF declaration text so Windows CRLF checkouts do not create false mismatches. The exact failing Windows digest was reproduced locally with a temporary CRLF checkout, then both LF and CRLF cases passed. Corresponding-source and runtime file checks remain raw-byte strict. The full eleven produced-image/contract tests passed again after this portability repair.

[Current final-UI core captures](captures/final-linux-core/manifest.json) and the README gallery now record the actual f599000 core, including visible hover and held scrollbar feedback. These fourteen images preserve their diagnostic-core provenance, source and hashes, while the latest hosted package run remains separately tracked.

## Owner review of the corrected build

Keep these unchecked until the owner has inspected the running app. Automated or developer visual checks cannot approve these items.

- [ ] New SSH/RDP session: the initial page shows the essential connection fields; advanced, network, terminal and bookmark options are easy to find, and switching sections keeps entered values.
- [ ] Scroll long forms and pages: the scrollbar stays at the right edge of its viewport, clears the content and visibly responds to hover and drag. Resize the window and repeat.
- [ ] Terminal and sidebar: everyday actions remain easy to reach, with profile/history/layout settings and longer guidance available on demand.
- [ ] Settings, games and detached terminal: spacing, navigation, backgrounds and action placement feel consistent at normal and smaller sizes.
- [ ] Light, Dark and System: controls, text, focus, hover and selection remain readable; explorer icons distinguish folders and file types.
- [ ] Keyboard and safety: navigate the new-session form, dismiss nested dialogs and cancel closing active work without losing the session or draft.
- [ ] Owner explicitly approves merging PR #8 after completing the checks.


## Owner review of the combined bar and shared sidebar

- [ ] One top row at normal and small sizes: app commands, connection tabs, theme, Find/Reconnect and the appropriate connection menu remain reachable.
- [ ] SFTP opens as a tab in the existing left sidebar; switching to Sessions and back preserves the connection, directory and selection.
- [ ] Tools, services, local/remote files, macros, tunnels, settings and document editors feel as clear as the approved session dialogs; secondary details stay on demand.
- [ ] Command/protocol/file icons, hover, pressed/focus states and tooltips remain clear in Light, Dark and System.
- [ ] Browser and detached-window navigation work without losing input or drafts; destructive actions and pending work remain protected.
- [ ] Owner completes the full checks and explicitly approves merging PR #8.
