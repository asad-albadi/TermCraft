# TermCraft remote workspace

Recreate the supplied remote-access desktop suite in Rust with a Fluent interface. Full feature parity on Windows, Linux and macOS is the acceptance target; the present build does not meet it. External adapters are partial implementations, not substitutes for the requested integrated engines. The user requested feature parity, cross-platform operation, and autonomous implementation. Only app/ contains deliverable changes. Investigation state is maintained by Craft Anything.

## Design
Preserve the original desktop information hierarchy: title and menu rows, an icon-over-label command toolbar, a narrow left navigation rail, a saved-session tree with quick connect, a tab strip above the working area, and a bottom status bar. The home tab is a quiet terminal start screen rather than a dashboard of promotional cards. Menus retain Sessions, View, X server, Tools, Games, Settings, Macros and Help. Unimplemented Games and Packages are explicitly unavailable; no feature parity is implied by retaining their positions.

Fluent visual tokens follow the supplied penguin logo: light accent #007857, text #182720, secondary text #606060, chrome #EFF5F2 and content #FBFDFC. Dark accent is #34D399, with #1B231F chrome and #131917 content. The bundled interface sans font supports compact command labels; the bundled monospace font remains dedicated to terminal content. Controls use 4 px corners, dialogs 8 px corners, subtle outlines and emerald selection accents. The 38 px rail and 226 px session pane keep the terminal dominant. Terminal tabs scroll horizontally as sessions accumulate. Existing connection, file, macro, tunnel and server workflows remain available through the familiar command groups. The supplied penguin logo appears in the native window icon, title row and home screen; utility icons remain independently drawn.

No original window capture is available, so this is a structural restoration guided by the requested MobaXterm hierarchy, not a verified pixel-for-pixel reproduction.

## Architecture
Rust eframe/egui UI, independent terminal screen model, platform process adapters, typed session configuration and JSON persistence. Unix terminal processes use a real pseudoterminal. Windows uses a platform adapter. Network protocols use installed platform clients where available; the person-choice tool instructed autonomous selection, so desktop/browser/X engines use installed system clients and show dependency errors when absent. Passwords are entered directly into the terminal, never saved. Session arguments are passed separately, never interpolated into a shell.

## Scope and evidence
Original functions inspected include entry 0xb521fc, session creation 0xaa7774, launch 0x8cc760, command generation 0x8d9830, terminal selection 0x97e2f4, SSH open 0x9182d4, tunnel limit 0x91fd40, SSH gateway 0x8c7f3c and SFTP connection 0x9e0720. Full fidelity is not established. Original capture could not run due to the host approval policy, and no reference screenshots exist. Visual design is an intentional Fluent reinterpretation. Backend dependencies and unsupported embedded engines are disclosed in the UI and final report.

## Verification
Test real argument construction and rejection, terminal cursor/erase/UTF-8/alternate-screen behavior, persistence and PTY shell I/O. Build offline using cached Rust libraries. Run --screenshot for about 120 frames, inspect saved PNG, then run Craft Anything rebuild_check. Record what can and cannot be compared; do not claim full parity from build success.

## Follow-up: full-parity requirement
The authoritative scope is FEATURE_PARITY.md, exposed in Help. Missing embedded engines,
plugins, remote browsing and platform integrations remain required work. Do not remove
these requirements to declare the project complete. Keep existing protocol and session
behavior while adding implementations; verify each platform independently.

The session pane now collapses through View, the Sessions command and Ctrl+Shift+B,
and resizes between 190 and 480 logical pixels. Visibility and width persist through
workspace preferences without invalidating older stores. The navigation rail remains
available when the pane is hidden. Saved SSH context menus open a companion SFTP tab
with the same endpoint and authentication options, without running the SSH startup
command. This does not yet reproduce the automatic graphical SFTP sidebar.

Original scope and layout references reviewed on 2026-11-23:
https://mobaxterm.mobatek.net/documentation.html and
https://mobaxterm.mobatek.net/features.html, including its original interface images.
These support reconstruction priorities, not claims of visual or behavioral equivalence.

## TermCraft appearance update
TermCraft uses a shared System/Light/Dark appearance preference. The supplied penguin icon anchors an emerald palette: #007857 accents on light #FBFDFC content and #EFF5F2 chrome, and #34D399 accents on dark #131917 content and #1B231F chrome. Tabs contain their close control and join the matching terminal surface with square lower corners and an emerald active marker. Hover, keyboard focus, text selection and search matches have distinct treatments. Search highlights only matching text; ANSI inverse colors resolve after theme defaults. Explicit application-supplied ANSI colors remain unchanged. Existing workspace config paths and the Rust crate identifier remain compatible with previous builds.

On Windows and macOS, System uses the native theme signal supplied by eframe. Linux winit 0.30 reports no system theme, so a background worker reads the XDG Desktop Portal's `org.freedesktop.appearance/color-scheme` setting every two seconds using zbus. Portal values 1 and 2 map to dark and light; no preference, errors and unknown values fall back to light. The worker requests a repaint when the preference changes and stops when the workspace closes. Portal I/O stays off the UI thread. Explicit Light and Dark selections take precedence. Regression tests cover absent native signals, desktop preference changes and explicit overrides; a separate opt-in test exercises a live portal.
