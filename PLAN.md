# Implementation plan

Current complete plan: [all 60 parity points](docs/superpowers/plans/2026-11-23-complete-parity-plan.md), with ten focused subsystem plans and shared contracts. The outline below records the earlier starter-app implementation; it is not the new execution checklist.

Goal: a fully functional cross-platform MobaXterm replacement with Fluent presentation. A working workspace alone does not meet this goal. FEATURE_PARITY.md tracks the remaining required scope.

1. Establish session models, safe protocol arguments, and settings persistence. Test ports, option injection and JSON round trips before implementation.
2. Implement terminal screen parsing and platform process I/O. Test line edits, escape sequences, Unicode, and an actual PTY shell. Use bounded background channels and clean up processes when tabs close.
3. Implement Fluent workspace, saved-session editing, folder/search, terminals, file operations, transfers, tunnels, macros, settings and diagnostics. Show missing engines as actionable errors. Keep unsafe shell interpolation out of connection forms.
4. Build, run screenshot export, inspect image, exercise local workflows, and fix findings. Screenshot mode must not focus the window and must exit after capturing a nonblank rendered image.
5. Run rebuild_check, fix actionable results, update unknowns and checklist, and disclose remaining parity gaps.

Review focus: malformed host inputs, keys with spaces, processes after closing tabs, partial UTF-8 and ANSI output, corrupt settings files, absent external clients, errors on read-only paths, screenshot mode without focus. No subagent delegation requested.

## Required unfinished work
- Implement embedded X11, RDP/VNC, graphical remote browsing/editing, plugins and packages.
- Complete protocol/session options, shared/imported sessions, terminal conformance,
  servers, password/agent management, layouts and platform integrations listed in the matrix.
- Validate native Windows, Linux and macOS operation, real protocol endpoints and original
  behavior; estimated defaults stay labeled until supported by evidence.

## This revision
- Added persistent sidebar visibility/width, keyboard/toolbar/menu toggles and resizable layout.
- Added SSH companion SFTP workflow preserving connection settings.
- Fixed IPv6 SSH tunnel address formatting and rejected unknown tunnel modes.
- Added feature matrix in the application and documentation, including defaults and evidence limits.
