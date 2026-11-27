# Fluent 1 and remote-workspace UX research

This document records the starting review. The [integrated verification record](validation.md) gives the final disposition and evidence for these surfaces.

Research date: 2026-11-27. Direction explicitly selected by the owner: **Windows 11 desktop Fluent, retaining the owner’s preference for Fluent 1 over Fluent 2**, preserving TermCraft's existing layout, penguin and emerald identity. Windows 11 is the explicit visual reference; the earlier inference that Fluent 1 required Windows 10/UWP geometry was corrected before dialog migration. It is a refinement pass, not a toolkit migration.

## Primary design references

| Source | Finding and TermCraft application |
| --- | --- |
| [Microsoft's 2017 Fluent Reveal introduction](https://blogs.windows.com/windows-insider/2017/06/21/announcing-windows-10-insider-preview-build-16226-pc/) | Historical primary reference for Fluent 1's pointer illumination. Apply restrained Reveal-inspired hover/pressed feedback to app navigation and commands, separate from persistent selection and keyboard focus. |
| [UWP RevealBrush](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.media.revealbrush) | Confirms lighting-based treatment; the native API is now discouraged. Reproduce the visual interaction in existing egui painting, rather than depend on obsolete Windows-only APIs. |
| [UWP NavigationView, Windows 10](https://learn.microsoft.com/en-us/archive/msdn-magazine/2017/december/universal-windows-platform-a-developer%E2%80%99s-guide-to-the-new-hamburger-menu-in-windows-10) | Adaptive navigation with consistent selection, input and accessibility. Preserve the rail and session tree, improve hit areas/focus and responsive overflow. |
| [UWP ContentDialog](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.controls.contentdialog) | Fluent 1's modal control model: meaningful title, bounded content, explicit actions and keyboard behavior. Use actual modal blocking for confirmations/authentication and deliberate modeless editors where appropriate. |
| [Dialog controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs) | Action-specific labels, safe close action, primary actions before the safe action, inline contextual validation. Do not let scrolling hide the footer. |
| [Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic) | Depth belongs on suitable supporting/transient surfaces. Avoid layering noisy translucent panes or placing unreadable accent text on them. Solid fallbacks are required; egui's current cross-platform renderer has no native desktop-blur contract. Do not claim a translucent tint is actual acrylic blur. |
| [Content spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics) | Consistent small spacing increments, explicit grouping and type hierarchy. Use a 4-point grid, 8 between related controls, 16/24 for page/dialog grouping. Exact TermCraft values are engineering choices, not claimed universal Microsoft mandates. |
| [Alignment and padding](https://learn.microsoft.com/en-us/windows/apps/develop/ui/alignment-margin-padding) | Responsive constraints and readable insets; fixed widths must not clip labels, long paths or footer controls. |
| [Scroll viewers](https://learn.microsoft.com/en-us/windows/uwp/controls-and-patterns/scroll-controls) | Thin resting indicator expands for pointer manipulation; reserve clearance so the scrollbar cannot cover content. Keep wheel/keyboard/drag usable and avoid scroll-layout jumps. |
| [Timing and easing](https://learn.microsoft.com/en-us/windows/apps/design/motion/timing-and-easing) | Platform references include 83/167/250 ms durations. Use short feedback transitions without delaying actions or moving hit targets; provide reduced motion. |
| [Keyboard accessibility](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/keyboard-accessibility) | Keyboard is a primary interaction: visible focus, reading-order traversal and reachable controls. Modal focus must not leak to remote terminal or desktop input. |
| [Accessible text](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessible-text-requirements) | Normal text must maintain at least 4.5:1 contrast. Add symbols/text to status; do not rely solely on color. |

Some historical UWP design URLs now redirect to current Windows guidance. Historical 2017 references explain the first-generation lighting concepts only. The owner subsequently corrected the OS visual reference to Windows 11. Current Windows desktop geometry and material rules therefore govern the appearance; Windows 10 geometry is not the target. No Fluent 2 web-toolkit migration is involved.

## User feedback and requests

These are individual user reports and discussions, not prevalence measurements or independently reproduced MobaXterm defects. We use the original reports to identify usability questions; suggested answers are not automatically adopted.

| Original discussion | Reported experience | Resulting audit requirement |
| --- | --- | --- |
| [Reddit: font changes inconsistent between terminals](https://www.reddit.com/r/mobaxterm/comments/mz2c0r/) | Saved-session overrides are confused with global defaults and newly opened terminals. | Explain global versus per-session and per-tab settings, inheritance/reset and when changes apply. Preserve existing overrides. |
| [Reddit: overwrite font size for all sessions](https://www.reddit.com/r/mobaxterm/comments/zi0g8r/) | A global font/color adjustment is hard to discover; responses suggest editing an INI file. | Make terminal profile configuration discoverable; do not require configuration-file editing for normal appearance controls. |
| [User report in powerlevel10k](https://github.com/romkatv/powerlevel10k/issues/2599) | Installing a font alone does not update existing bookmarked-session overrides. | Keep inherited/overridden values explicit rather than silently altering session definitions. |
| [Reddit: detached tab sizes](https://www.reddit.com/r/mobaxterm/comments/1lpbiui/how_to_set_the_size_of_a_detached_tab/) | Repeatedly resizing detached windows on ultrawide displays is tedious. | Retain existing saved geometry and ensure chrome/form layout works in detached/resized windows. Do not replace geometry with screen-percentage defaults. |
| [Stack Overflow: right mouse button double click](https://stackoverflow.com/questions/51622536/mobaxterm-right-mouse-button-double-click/75209731) | Context-menu and right-click paste expectations differ. | Make the current right-click mode and clipboard shortcuts discoverable while preserving terminal mouse-reporting and paste safeguards. |
| [Super User: change copy shortcut](https://superuser.com/questions/1844885/cant-change-copy-shortcut-in-mobaxterm) | Users cannot find expected copy behavior or distinguish local shortcuts from terminal input. | Accessible shortcut labels, clear Copy/Paste actions, no interception of legitimate remote Ctrl+C. |
| [Super User: terminal settings not persisted](https://superuser.com/questions/1824076/mobaxterm-terminal-settings-will-not-get-saved-in-subsequent-runs-of-a-session/1824078) | Repeated appearance changes appear not to survive reconnect. | Preserve persistence regressions and clarify scope rather than changing storage semantics for a visual fix. |
| [Reddit sysadmin discussion](https://www.reddit.com/r/sysadmin/comments/canr0o/) | Participants value combined tools, organized hosts, tabbed work and MultiExec; some dislike inconsistent ancillary windows. | Preserve familiar organization and safety-visible MultiExec target selection; apply the same design vocabulary to every ancillary window. |

## Decisions

Use Windows 11 desktop geometry (4px controls, 8px overlays, square docked edges), compact readable controls, consistent semantic emerald colors, bounded modeless tools and real modal confirmations. Preserve the terminal content renderer and remote protocol behavior. Effects must never obscure text, weaken trust dialogs, delay input or run perpetual idle animation. Use a solid material fallback rather than introducing an unverified blur pipeline. Native permission dialogs/file pickers and remote desktop/browser content are platform/remote-owned; their app-owned surrounding controls remain in scope.

## Windows 11 clarification — owner correction

The owner explicitly specified **Windows 11, not Windows 10**. The preference for Fluent 1 does not authorize substituting an older Windows 10 visual style. Apply [Windows 11 geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry): 4px controls/backplates, 8px dialogs/flyouts, square touching/docked edges. Use current desktop [material roles](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials): Mica for suitable long-lived backdrops, Acrylic for suitable transient surfaces, and Smoke for modal blocking. [Mica guidance](https://learn.microsoft.com/en-us/windows/apps/design/style/mica) explicitly includes solid fallbacks. Native backdrop integration is unavailable in the present egui rendering contract; preserve an honest solid fallback rather than calling a color tint Mica or Acrylic. Keep restrained input feedback, accessible focus and modern dialog composition.

### Typography implementation check

[Microsoft's Windows 11 typography reference](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography) distinguishes regular body text and semibold titles, and recommends open-source [Selawik](https://github.com/microsoft/Selawik) for cross-platform use without redistributing Segoe UI. Local inspection found that egui0.36.2's `RichText::strong()` changes text color only, and TermCraft's current embedded interface face is Liberation Sans Regular. The specialist refinement batch must supply real font-weight hierarchy with appropriate licensed faces, preserve terminal font choices and document platform fallbacks.

### Command actions in dialog footers

The current [Microsoft buttons guidance](https://learn.microsoft.com/en-us/windows/apps/design/controls/buttons) recommends a 120px minimum for short command labels and responsive resizing, wrapping or stacking for longer labels. A single safe dialog close action is right-aligned. [ContentDialog guidance](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs) places the primary action leftmost, optional secondary action in the middle and safe close action rightmost. Apply these roles to app-owned dialog footers while retaining operation-specific safe defaults and input guards; navigation/command-bar tiles and inline icon actions are separate roles.


## Human-review correction: session composition

The owner rejected the prior implementation's density and scrolling on 2026-11-27. Initial reference research and automated layout tests had not established a simple session-creation experience.

The correction directly inspected the [MobaXterm session-creation screenshot in NeSI's setup guide](https://docs.nesi.org.nz/Getting_Started/Accessing_the_HPCs/MobaXterm_Setup_Windows/) and checked the [official session documentation](https://mobaxterm.mobatek.net/documentation.html). The screenshot shows protocol choices across the top, a compact basic SSH row for host, optional username and port, with separate advanced, terminal, network and bookmark tabs. This supports a change in information organization, while TermCraft retains its Windows 11 styling and its existing connection/safety behavior. It is not evidence that a proposed TermCraft layout is already usable; native normal, ultrawide and compact previews and owner acceptance are still required.

The owner's screenshots also expose naturally shrinking scroll viewports, permanently expanded explanatory text, redundant terminal controls and a games surface that does not occupy its window. The [correction checklist](human-review-correction.md) tracks these as reopened work.
