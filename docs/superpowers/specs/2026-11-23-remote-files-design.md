# F01 — Embedded SSH and automatic remote file pane

Status: approved by the user on 2026-11-23; implementation plan pending review. Date: 2026-11-23.

## Intent and authority

The user authorized working through all 60 items in `PARITY_CHECKLIST.md` in order, reporting progress and retaining honest verification status. F01 is first: automatically open a graphical remote directory pane with an SSH connection.

The user selected **Embedded SSH/SFTP engine** when offered that approach versus retaining installed OpenSSH clients. The user subsequently approved this builder-reviewed design and instructed continuation. The expanded full-parity plan is a separate review artifact.

Only files inside `app/` are deliverables. The directory currently has no Git repository, so this design is saved in place; no commit or worktree can be reported.

## Existing behavior

`src/connection.rs` builds installed `ssh`/`sftp` command arguments. `src/ui.rs::TerminalTab` owns a PTY/ConPTY `Process`; `src/ui/pages.rs` sends transfer commands to an independent interactive SFTP tab. SSH and transfer sessions authenticate independently. There is no structured remote-directory model or remote file pane. The local file page operates on local paths.

Current SSH options include host, port, username, private-key path, jump host, trusted X11 forwarding, compression, startup command, and a fixed keepalive interval. Existing session JSON must continue loading. Passwords are not stored. The penguin branding, emerald palette, System/Light/Dark behavior, local terminals, and other protocols must remain functional.

Baseline: 30 automated tests passed on 2026-11-23; one live desktop-portal test is opt-in and was not rerun during this baseline.

## Approaches

1. **Embedded SSH/SFTP, selected by the user.** A library-owned connection carries terminal and transfer channels. This enables structured listings, shared authentication, cancellation, and later remote editing. It requires migrating connection lifecycle, authentication prompts, and host verification into TermCraft.
2. **Installed OpenSSH plus a structured SFTP subprocess.** Preserves SSH configuration behavior, but shared interactive authentication and Windows multiplexing complicate the automatic browser. Rejected in favor of the embedded direction.
3. **Parse the interactive SFTP terminal's display.** Not suitable: filenames, prompts, localization, and terminal escape output are not a reliable structured filesystem API. Not offered as a viable implementation.

## Scope of the first subsystem

F01 establishes the embedded connection and graphical directory listing. F02 through F06 remain distinct checklist outcomes and are addressed next. The embedded connection must support independent shell and SFTP channels under the same successful authentication; the broad F06 completion claim additionally requires its reconnect and prompt behavior to be verified.

The first usable increment supports direct SSH connections, with keys, agents where supported, password authentication, and keyboard-interactive prompts. A subsequent F01 compatibility increment covers remaining currently supported SSH profiles. F01 must not be checked complete merely because the direct-connection increment works.

The complete F01 connection profile set is bounded to explicit TermCraft fields: direct host/port/user connections, supported private keys/agent/password/keyboard-interactive authentication, the existing single jump-host field, compression, trusted X11 forwarding, and startup commands. These explicit options must not be silently ignored. Arbitrary OpenSSH configuration directives and externally supplied proxy programs are not additional F01 completion requirements: retain those workflows in labeled compatibility mode, and track their migration under S01/S07. This avoids treating the unbounded OpenSSH configuration language as an implicit prerequisite for the first feature. Unsupported explicit fields keep F01 partial until their accepted behavior is implemented and verified.

For existing jump-host, X11, and custom OpenSSH-configuration workflows that are not yet supported by the embedded engine, retain an explicitly labeled **OpenSSH compatibility mode**. It must disclose that the automatic remote pane is unavailable in that mode and offer the existing companion SFTP workflow. Do not silently discard options, fall back to a different endpoint, or imply complete F01 coverage. These profiles remain F01 compatibility work, even where later checklist items add broader integration.

F01 does not add transfers, delete/rename operations, remote file editing, shell-directory following, an SCP browser, a password vault, or other protocols. Navigation by opening a directory, parent navigation, refresh, and retry are included because they make a directory pane usable. File rows are read-only at this stage.

## Proposed architecture — builder decisions, not user specifications

Use Rust `ssh2` (libssh2) as the initial embedded engine candidate. Its API supports authenticated sessions with terminal and SFTP channels, known-host checking, public keys, agent authentication, passwords, and keyboard-interactive callbacks. Confirm the dependency's supported algorithms and target build requirements before choosing an exact version. If it cannot satisfy the compatibility increment without regressions, return with the concrete finding rather than substituting a weaker backend silently.

Keep transport and UI separate:

- `src/ssh/`: connection lifecycle, authentication challenges, host-key checking, terminal I/O, cancellation, and typed connection events.
- `src/remote_files/`: remote paths, directory entries, listing/navigation state, and typed operations/results. No local-filesystem interpretation of remote paths.
- `src/ui/remote_files.rs`: graphical listing and connection/challenge controls using the existing visual tokens.
- `TerminalTab`: owns the connection handle and per-tab remote-file state. It dispatches terminal input/resizes through the selected transport and renders ordinary terminal output through the existing `Screen`.
- `Process`: remains the local-terminal and compatibility-client transport. Avoid unrelated UI restructuring.

One worker owns each embedded SSH session and schedules both channels. Session-wide blocking calls must not prevent cancellation or indefinitely starve terminal input. Network I/O never runs on the egui render thread. Use bounded queues; handle queue saturation explicitly. Closing a tab cancels pending challenges and requests, closes both channels, and releases transport resources. A reconnect creates a new connection generation; results from a previous generation must never update the new tab.

The library's documented session-wide synchronization means simultaneous blocking calls on separate cloned handles are not sufficient for concurrent I/O. The implementation plan must choose a nonblocking scheduling strategy and tests for an idle shell during listing and terminal input during slow SFTP responses.

## User-visible behavior

1. Starting an embedded SSH tab immediately shows a remote-files area beside its terminal, initially labeled Connecting.
2. Host verification and authentication challenges are visible in that tab. Loading the pane never creates a second login request after authentication succeeds.
3. After authentication, open the terminal and an SFTP channel, resolve the remote starting directory, then show entries and the current remote path.
4. Opening a directory navigates there; Parent, Refresh, and Retry operate on that connection. A refresh preserves the last successful listing until replacement data arrives.
5. Changing tabs changes the displayed directory state. Collapsing the ordinary saved-session pane must not disable the remote browser. The browser can be shown/hidden and resized independently.
6. If the server does not provide SFTP, show a file-pane error and a retry control while keeping the SSH terminal usable. Do not disconnect a working shell just because browsing fails.
7. A connection error, authentication cancellation, or tab closure terminates pending work. Reconnect clears stale authentication and listing state.

Preserve readable filenames containing spaces, quotes, newlines, and Unicode without treating them as commands. Display symlinks distinctly; report errors when a navigation target cannot be resolved. The UI must never concatenate a remote filename into a shell command for listing/navigation.

## Authentication and host identity

- Check the presented host key before sending any credentials.
- Match supported OpenSSH known-host entries using the endpoint's hostname and port. Unknown hosts require an explicit fingerprint/trust decision; rejecting the decision cancels the connection.
- A changed host key stops authentication and explains the mismatch. Do not silently replace a known key or offer an automatic bypass.
- Keep trust storage separate from saved session settings and document its location. Do not rewrite the user's existing known-host files merely to add app trust.
- Show username, private-key passphrase, password, and keyboard-interactive challenges through transient per-tab controls. Respect challenge echo flags. Never serialize credentials or include them in logs, macros, or terminal search/export.
- Agent/key failure may lead to an explicitly supported alternate authentication method; cancellation must stop the attempt, not start another prompt automatically.
- Record that imported OpenSSH configuration is not yet equivalent to native OpenSSH behavior. Unknown configuration directives must not silently change connection identity or proxy routing.

## Acceptance and evidence

The implementation needs meaningful regression tests before production changes and real end-to-end tests against an isolated, temporary local SSH/SFTP server. Test fixtures own their keys, known-host data, files, listener, and processes. They must not change the user's SSH configuration or start a persistent service.

Required behavior coverage:

- Authenticate once; interact with the terminal and list the remote home directory over the same connection.
- Reject a changed host key before the server accepts credentials; reject or cancel an unknown host.
- Navigate directories with spaces, Unicode, and command-like filenames using structured SFTP operations.
- Keep the shell usable when the SFTP subsystem is absent or a listing is denied.
- Retain responsive input/cancellation with a slow listing, a stalled server, and a full queue.
- Close/reconnect during a challenge or listing; release resources and discard stale results.
- Render the pane in both palettes and when the saved-session pane is collapsed; preserve other workspace pages.
- Load old session JSON, retain existing launch options, and visibly route unsupported profiles to compatibility mode without claiming F01 complete.
- Check Linux builds and the Windows compile target. Native Windows/macOS behavior remains unverified until run there.

F01 status may move from Missing to Partial after the direct-connection increment. It is complete only after the complete accepted profile scope, UI outcomes, and supported-platform evidence meet the checklist's completion rule. Lack of hardware/platform evidence is recorded as pending validation, not a passed test.

## Builder review resolution

An independent builder asked which OpenSSH configuration behaviors bound full F01 completion. The required profile set is now explicit above. Advanced external-client configuration remains available through labeled compatibility mode and is tracked under S01/S07; it does not expand F01 into an unlimited OpenSSH clone. This is a builder decision for user review, not an additional user-approved requirement.

## Next work after this design

After design approval, write a focused implementation plan for the first F01 increment, including exact interfaces, regression tests, integration fixtures, and the nonblocking I/O strategy. Execute that plan, review it, record evidence, then continue the remaining F01 compatibility increment and F02 onward. Report the number of fully completed checklist points separately from subtask progress.

## Technical references

- [ssh2 Session API](https://docs.rs/ssh2/latest/ssh2/struct.Session.html): multiplexed channels and session-wide blocking implications.
- [ssh2 KnownHosts API](https://docs.rs/ssh2/latest/ssh2/struct.KnownHosts.html): host identity checks.
- [ssh2 keyboard-interactive API](https://docs.rs/ssh2/latest/ssh2/trait.KeyboardInteractivePrompt.html): server authentication challenges.
- Existing project requirements: `PARITY_CHECKLIST.md`, `FEATURE_PARITY.md`, `DESIGN.md`.
