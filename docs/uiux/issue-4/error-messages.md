# App-wide error messages and codes

The owner requested meaningful messages and stable codes throughout TermCraft. This pass routes application-owned errors through one catalogue and one compact UI: an explanation beside a `TC-…` code; opening the code shows the suggested next step, bounded technical details and an explicit **Copy error details** action. Ordinary progress, successful operations, terminal command output and user cancellations remain ordinary messages.

Existing connection forms, tabs, panels and confirmation flows retain their organization. The only presentation changes inside approved session dialogs are their error rows; validation stays alongside the form. No trust checks, overwrite consent, credential ownership, pending-operation cancellation or dirty-document close protections are bypassed.

## Code format

`TC-<feature>-<cause>` is stable across platforms. A code identifies a category, not a unique incident or source line. The original provider explanation remains in the report, after redaction. Unrecognized or localized errors use `000`; the app does not infer a root cause from an OS error number or invent a more specific explanation.

| Cause | Meaning |
| --- | --- |
| `000` | Unclassified error in the named feature; see the explanation/details |
| `101` | Connection refused |
| `102` | Operation timed out; a remote mutation may already have happened |
| `103` | Host-name resolution failed |
| `104` | Authentication rejected |
| `105` | Access denied or read-only destination |
| `106` | Required file/folder missing |
| `107` | Destination already exists |
| `108` | Storage space or quota exhausted |
| `109` | Connection ended unexpectedly |
| `110` | Busy resource, full queue or occupied listen address |
| `112` | Certificate verification failed |
| `113` | Saved server identity changed |
| `114` | Graphical display could not be opened |
| `115` | Unsupported operation or provider option |
| `116` | Invalid settings/input reported by the operation |
| `117` | Remote-file version conflict |
| `118` | Network/route unavailable |
| `119` | Local bundled component unavailable (RDP helper or verified browser runtime) |
| `120` | Local RDP helper exists but cannot start |
| `121` | RDP server explicitly rejected sign-in |
| `122` | RDP sign-in blocked by an account/server policy |
| `123` | Transport failed during RDP sign-in |
| `124` | Secure RDP sign-in protocol could not complete |
| `125` | Transport failed during remote desktop setup |
| `126` | Remote desktop setup protocol could not complete |
| `127` | Older helper omitted the failing sign-in/setup stage |

Examples: `TC-SSH-101` asks the user to check the host/port and whether its service is running; `TC-START-114` asks the user to launch from an available graphical desktop; `TC-TRANSFER-117` asks them to compare local and remote versions before replacing anything. Certificate/host-key errors require verifying identity, never weakening verification.

## Feature coverage

| Prefix | Application error boundaries |
| --- | --- |
| `APP` | General workspace errors |
| `START` | Graphical startup and working-directory/instance initialization |
| `CLI` | Command arguments and launch dispatch |
| `SETTINGS` | Workspace storage, damaged settings and backup |
| `SESSION` | Connection form validation, group launches and generic terminal controls |
| `SSH` | SSH connections/options and related remote-terminal failures |
| `RDP`, `VNC` | Remote desktop settings and active-view errors |
| `FILES` | Local files, editors, screenshots and terminal-output/log exports |
| `TRANSFER` | SSH/SFTP/FTP browser, transfers, remote editors, conflicts and recovery warnings |
| `SERVER`, `X11` | Managed services, X server, graphics probes and XDMCP |
| `KEYS`, `VAULT` | SSH keys/agents and credential operations |
| `MACRO`, `TUNNEL` | Macro operations and SSH tunnels |
| `IMPORT`, `SHARED` | Imports and shared-session source operations |
| `BROWSER` | Embedded browser settings, engine and input failures |
| `GAME` | Game configuration, file and operation warnings/failures |
| `PLUGIN`, `PACKAGE`, `TOOLS` | Plugins, runtime packages, file/network tools and Windows registry/integration tools |
| `CLIPBOARD`, `SHORTCUT`, `SCHEDULER`, `HIGHLIGHT` | Clipboard/paste, shortcuts, scheduler and highlighting |

Status errors carry their feature context and remain distinguishable from progress/success/cancellation. Background failures previously posted as ordinary status text now use error notifications. SSH failure details retain the original TCP/OS explanation, and automatic resizing no longer overwrites that explanation after a connection worker has finished. The remote file pane describes a disconnected browser instead of calling an unqueried directory empty.

## Details and copying

Reports include the code, readable explanation, suggested next step and bounded technical detail. Recognized passwords/passphrases, tokens, authorization headers, URL passwords, private-key blocks and secret command flags are redacted before display/copy. Source state and credentials are not rewritten. Opening a code never copies data automatically, sends a report or starts a corrective action. No new diagnostic upload or persistent error log is introduced. Providers and internal helper protocols retain their own error payloads; the application adds codes at the human-facing boundary.

The primary implementation is `src/diagnostics.rs` with the shared UI in `src/ui/errors.rs`. Unknown provider errors are intentionally retained rather than guessed. Add new classifications without renumbering existing codes, and test their recovery advice and redaction.

## Acceptance

- [x] Shared catalogue, stable feature/cause identifiers and bounded reports.
- [x] Common Unix/Windows reported causes and unknown-error fallback.
- [x] Error-code popup, next steps, explicit copy and credential redaction checks.
- [x] Initial complete connection-form and FTP editor protection checks.
- [x] Audit ordinary status versus actual failure notifications.
- [x] Final current-source native connection-cause and rendered-error verification.
- [ ] Owner human checks and explicit merge approval.

PR #8 remains draft and unmerged. Local native previews use the documented cached pre-adapter GUI dependencies with a freshly compiled current library; they are separate from full hosted platform/package qualification.

## Verification record

- Catalogue tests: **9/9**, including Windows/Unix cause consistency, numeric-code ambiguity, unknown providers, bounded reports, redaction and trust-safe advice.
- Final scoped UI/connection run: **15/15**, including explicit copy/redaction, error-versus-cancellation status, a refused real loopback SSH endpoint, resize preservation of its cause, graceful remote exit, connection forms, clipboard target ownership, credential generation ownership and dirty remote-editor protection. The run also contains the retained font/cursor and FTP checks.
- Packaging checks: **61 run, 21 skipped, 0 failures**; skipped cases retain their external/platform requirements.
- CLI: invalid arguments retain exit status 2 and report a `TC-CLI-…` code; unavailable graphical display returns nonzero with a `TC-START-…` code.
- Native UI: actual local refused SSH operation, code/detail popup and form validation observed. Sample bookmarks and isolated review data were used. Source/binary identity and screenshots are recorded with the native gallery.

The fresh current library is checked against its source hashes; GUI dependencies come from the previously documented pre-adapter cache. Full hosted platform CI is separate and runs on the pushed branch. No production package is relabelled as this local review build. Close the older app before using the updated local review launcher:

```bash
"/home/asad/.codex/worktrees/termcraft-issue-3/app/target/issue-4/run-terminal-row-review"
```

## Integration inventory

Presentation/notification call sites by source file (including feature-specific status boundaries; runtime tests are listed above):

| Source | Calls |
| --- | ---: |
| `src/ui.rs` | 4 |
| `src/ui/broadcast.rs` | 1 |
| `src/ui/chrome.rs` | 4 |
| `src/ui/cli_dispatch.rs` | 1 |
| `src/ui/clipboard.rs` | 4 |
| `src/ui/credentials.rs` | 5 |
| `src/ui/desktop.rs` | 1 |
| `src/ui/ftp.rs` | 7 |
| `src/ui/games/play.rs` | 2 |
| `src/ui/highlighting.rs` | 1 |
| `src/ui/http_share.rs` | 3 |
| `src/ui/macro_events.rs` | 4 |
| `src/ui/managed_cron.rs` | 2 |
| `src/ui/managed_servers.rs` | 1 |
| `src/ui/managed_ssh_iperf.rs` | 2 |
| `src/ui/managed_vnc.rs` | 2 |
| `src/ui/packages.rs` | 1 |
| `src/ui/pages.rs` | 25 |
| `src/ui/plugins.rs` | 1 |
| `src/ui/rdp_settings.rs` | 1 |
| `src/ui/remote_editor.rs` | 7 |
| `src/ui/remote_files.rs` | 3 |
| `src/ui/session_confirmations.rs` | 7 |
| `src/ui/session_groups.rs` | 5 |
| `src/ui/session_import.rs` | 1 |
| `src/ui/shared_sessions.rs` | 7 |
| `src/ui/shortcuts.rs` | 2 |
| `src/ui/ssh_keys.rs` | 1 |
| `src/ui/ssh_options.rs` | 3 |
| `src/ui/terminal_settings.rs` | 1 |
| `src/ui/tools.rs` | 5 |
| `src/ui/web_browser.rs` | 3 |
| `src/ui/windows.rs` | 1 |
| `src/ui/windows_platform.rs` | 1 |
| `src/ui/xdmcp.rs` | 2 |
| `src/ui/xserver.rs` | 4 |

## RDP review-build correction

The first local UI review directory omitted the RDP helper. Development lookup also assumed the source-tree helper existed, so the application reported a generic missing-file error when opening RDP. Development builds now prefer an adjacent helper, falling back to the source-tree build only when no adjacent component is present. Release lookup and manifest verification remain in force.

The corrected local review directory contains both the current GUI and `termcraft-rdp-engine`, plus notices. The helper is the previously qualified Linux component from source `05a2234`, artifact `15246857366`; its package-manifest SHA-256 was verified and all engine sources, lockfile and referenced options/wire/loader inputs remain unchanged. It is not relabelled as a newly compiled engine. Any later review-bundle refresh must retain the matching helper.

Missing helpers produce `TC-RDP-119` with a local-build explanation; existing helpers that fail to launch produce `TC-RDP-120`. A missing loader is distinguished from a missing executable. Missing CA files retain their separate error classification.

Validation: resolver/error-format tests **4/4**, updated catalogue **10/10**, scoped UI **10/10**, helper startup/private protocol **PASS**, and **4/4** endpoint checks for actual adjacent lookup, frame/input, certificate-change resolution and transport/helper shutdown. The independent endpoint fixture was rebuilt from current source after an old local fixture lacked the current CA layout; TLS checks and endpoint assertions were retained. The review launcher uses the new directory and preserves Wayland. Full platform/release qualification and owner human checks remain separate. PR #8 stays draft and unmerged.


## RDP sign-in follow-up

The owner reported a failure immediately after submitting the password. The prior helper discarded the connector error and combined secure sign-in with desktop setup, so the reported message did not establish whether credentials were rejected. It also appeared twice because both the workspace/detached wrapper and the remote desktop pane rendered the copied error.

The remote desktop pane now owns its failure display. Attached and detached wrappers defer to it; failed connections no longer paint “Waiting for the remote desktop”. Successful or pending sign-in forms retain their existing layout.

The helper now reports the secure sign-in and desktop setup stages separately, retaining TLS/NLA verification and a shared negotiation deadline. Diagnostic details use locally defined labels, numeric SSPI/NTSTATUS/RDP values and pinned implementation locations. They omit raw provider descriptions, account identities, passwords, tokens and packet data. Unknown status values remain protocol failures; they are not guessed to mean a bad password. Desktop-setup access denial retains its desktop stage instead of being mislabeled as a sign-in rejection.

Use a plain username with the separate Domain field, or leave Domain blank when using `COMPUTER\username`, `DOMAIN\username` or an account UPN. Combining a qualified username with a separate domain can fail locally before server authentication.

- [x] Single attached/detached desktop failure owner, with an actual workspace rendering regression.
- [x] Seven stage-specific diagnostic categories, existing redaction and unknown-cause safety retained.
- [x] Independent wrong-password endpoint reports the exact safe stage, SSPI and peer NTSTATUS, without credentials.
- [x] Successful authentication, remote input, resizing and ignored-resize fallback remain covered.
- [ ] Owner retry against the actual Windows endpoint and identification/resolution of its reported cause.

The Windows host failure remains undiagnosed until the updated helper observes a retry. The wrong-password test validates the diagnostic path and does not prove that the owner's password was wrong. Close the older app, run the current review launcher and reconnect. If it still fails, open the code and copy its technical details. PR #8 remains draft and unmerged.

Final local checks: 11 real UI/input/ownership tests, 11 catalogue/redaction tests and 8 independent RDP/helper checks passed, including failed-password stage/status, successful sign-in, ordered input through resizing, ignored-resize fallback, certificate changes and shutdown. Six helper unit tests passed separately. Final native Wayland startup and protected-dialog checks passed. [Review component and source identities](rdp-signin-review-build.json).


## Bundled browser review correction

The local review refresh carried the GUI and RDP helper but omitted the browser resource directory. Its failure was local; changing the website address could not repair it. The review now includes the exact pinned Chromium tree (243 file hashes verified against the compiled manifest), notices and the unchanged current RDP helper. `TC-BROWSER-119` identifies the provider's unavailable/damaged-runtime result and directs the user to a complete local build. Verification and sandbox requirements remain enforced; there is no silent provider substitution.

Current checks: 12 catalogue/redaction tests, 9 UI/input/vault tests and the native bundled-renderer namespace/seccomp/private-profile cleanup qualification all passed. The RDP UI test now enters the synthetic password through the focused widget using both text and paste events before submitting it, instead of assigning its internal value. Native Linux startup passed. [Current review source/component identities](browser-review-build.json).

Keep the browser tree and RDP helper alongside the GUI on future review refreshes. The stable local review launcher selects the new payload; older directories remain intact. The new directory covers these two providers, not every production runtime.

The owner's real Windows rejection remains under investigation. Nine same-identity, same-certificate comparisons across earlier/current helper revisions passed on an owned endpoint; they do not qualify the owner's Windows host. For a local account, the account domain is the Windows computer name rather than the network workgroup. Existing event4625's target account/domain and SubStatus can distinguish failures without guessing the password. [Microsoft event4625 field reference](https://learn.microsoft.com/en-us/windows/security/threat-protection/auditing/event-4625).
