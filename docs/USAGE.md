# Using TermCraft

For installation, platform status and builds, see the [README](../README.md).

### Your first session

Drag to select terminal cells; **Alt-drag** selects a rectangle. **Ctrl+Shift+C/V** copy/paste. Clipboard settings include copy-on-select, word delimiters and right-click paste; **Shift-right-click** keeps the menu reachable.

GUI clipboard reads stay bound to the original terminal targets while running in the background. On Wayland, the optional `wl-paste` helper is preferred; native provider errors are reported and keyboard paste remains available.

Multiline paste and closing active sessions ask for confirmation by default. Settings also offers a per-line paste delay. Broadcast paste confirms its original targets together; Cancel sends no paste payload. A running paste can cancel its remaining input.

Configure font, size, light/dark palettes, encoding, Backspace and fixed/dynamic dimensions in **Settings → Configure global terminal profile**. Sessions and open tabs can override individual values or reset to global. Supported encodings are UTF-8, Windows-1252 and Shift_JIS; changes affect future input/output while preserving raw logs and retained text.

Configure keyword colors and bounded literal/regular-expression rules in **Settings → Configure terminal highlighting**. Saved sessions and open tabs can inherit, enable or disable them; search and selection take visual priority.

1. Choose **Start local terminal** on the home screen to open your default shell.
2. Choose **Create a new remote session** to configure and save a connection.
3. Double-click a saved session in the sidebar to connect.
4. Right-click a saved SSH session to open a companion SFTP tab with the same connection options.

Install `ssh` and `sftp` on `PATH` for OpenSSH compatibility mode and standalone SFTP tabs. New embedded SSH sessions use the in-process engine. If a required client is missing, TermCraft reports the dependency and keeps the session settings so you can reconnect later.

## Using the workspace

- **Sessions:** use the Sessions toolbar button or **Ctrl+Shift+B** to show or hide the session pane. Drag its edge to resize it; width and visibility persist.
- **Import sessions:** choose **Sessions → Import MobaXterm/PuTTY** to preview supported SSH definitions before applying them. Moba bookmark INI/documented CSV and PuTTY Registry exports are supported with explicit unsupported-field warnings and duplicate policies. Credentials and source commands are excluded; original/native format validation is still pending.
- **Terminals:** open multiple tabs, choose resizable two-column, two-row, or four-pane layouts, detach/reattach tabs through their context menu, rename them, search output, or export and log a session. MultiExec lets you select connected targets, view them in paged four-pane layouts, and broadcast accepted keyboard input or confirmed paste. Newly opened tabs stay excluded; failed targets are reported. Configure application and terminal-history shortcuts under **View → Keyboard shortcuts**. Inactive tabs show **new output** separately from running/error status.
- **Transfers:** use the embedded remote pane for file/folder upload and download, explicit overwrite conflicts, progress and cancellation. Drop local items onto its upload zone, or drag remote entries onto the chosen local download folder. Standalone companion SFTP tabs may authenticate again.
- **Remote folders:** enable **Follow shell directory changes** to follow OSC 7 notices. Expand **Shell setup** to copy reversible Bash, Zsh or Fish setup; paste it into that SSH shell intentionally. Without compatible notices, use the manual folder field.
- **Remote editing:** double-click a regular file in the remote pane to edit UTF-8 text up to 2 MiB. Save writes a local recovery snapshot before uploading. A changed remote version produces a conflict with an explicit replacement choice; closing dirty documents offers Save, Discard or Cancel.
- **Files:** browse and edit local text files, copy files without overwriting an existing destination, rename items, and delete files or empty directories.
- **Tunnels and tools:** configure local, remote, or dynamic SSH forwarding; use DNS lookup, ping, SSH-port checks, and SSH-key generation.
- **Appearance:** select **System**, **Light**, or **Dark** from the title bar or Settings. The penguin logo and emerald accents carry through both palettes. System mode follows the native desktop theme; on Linux it reads the desktop appearance portal every two seconds and defaults to light when no preference is available.

The remote pane supports directory navigation, mkdir/rename, confirmed deletion and staged transfers. Explicit folder overwrite merges complete files while preserving unrelated entries; cancellation or failure can leave earlier files completed and retains remaining staging data. Absolute and parent-relative symlink targets are refused. Directory following and remote editing have Linux implementation tests; native GUI and Windows/macOS validation remain pending. Remote documents stay bound to their opening SSH connection. After reconnecting, choose **Compare recovery in active SSH session** from the retained editor. Matching authenticated usernames, endpoints and verified target/gateway keys permit importing your text into a successfully opened document without unsaved destination edits; Save remains a separate action. SFTP checks for remote changes before and after staging, but cannot prevent another writer racing the final rename.

Use **Sessions → Shared session sources** to add a local file, HTTPS endpoint, or SFTP file through a saved embedded SSH profile. Shared entries are read-only; refresh updates their source group, while **Copy to local** creates an independent saved session. Offline entries keep a labeled stale cache. HTTPS supports private Basic sign-in; SFTP shares the SSH host-key/authentication flow. Source commands and local resources require review before launch. No source password is saved automatically.

Right-click a saved-session folder to launch it and its subfolders; a results window reports immediate failures while valid peers connect independently. Session and folder menus can save launch shortcuts. Linux `.desktop` entries are tested through the desktop launcher; Windows `.lnk` and macOS `.command` shortcuts still need native validation. Existing shortcut files are never overwritten.

CLI actions include `--session <stable-id>`, `--group <folder>`, `--new-tab`, `--command <text>`, `--macro <name>`, and local `--cwd <directory>`. Use `--data-dir` for an isolated workspace. Requests for an already running workspace are acknowledged after queuing; connection results appear in its tabs. Commands and macros create their own target terminal. `--exit-when-done` closes those terminals when their processes finish and preserves existing user terminals. Run `--help` for all options.

Use **Sessions → SSH keys and agents** (also available in Tools) to create a new key bundle, inspect/import a private key, copy its public key, or manage agent identities. A bundle contains `private_key` and `public_key.pub`; existing destinations are never replaced. Generation supports Ed25519, ECDSA and RSA, with an optional passphrase. Protected OpenSSH imports keep their ciphertext; recognized PPK keys can be converted while retaining protection. The manager rejects oversized/high-cost key inputs and encrypted PKCS#8 conversion is currently unavailable.

Start a **TermCraft-owned** agent explicitly, add an identity with an optional lifetime, and choose it in an embedded SSH session's agent setting. Source-only SFTP uses that same selection. System agents remain borrowed and are never stopped by TermCraft. An explicit key file takes precedence; **Disabled** prevents agent use. Owned/Pageant selection is unavailable in installed-client compatibility mode. Windows/macOS agent UI/service/Pageant behavior still needs native validation.

## Settings and storage

Sessions, tunnels, macros, and preferences are saved locally as JSON. Existing Harbor configuration paths are retained so earlier workspaces continue to load:

| Platform | Default configuration directory |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/harbor`, or `~/.config/harbor` |
| macOS | `~/Library/Application Support/Harbor` |
| Windows | `%LOCALAPPDATA%\Harbor`, with the user profile as fallback |

Remote editor recovery snapshots are retained under `remote-recovery/` inside the settings directory. The editor displays the exact local path. Closing or discarding a document keeps these snapshots; remove copies when you no longer need them.

Choose an independent settings directory with:

```sh
cargo run -- --data-dir ./my-workspace
```

Passwords are not saved by default. Choose **Sessions → Credential vault** to create or unlock an encrypted vault, then select **Save entered value in vault** on an eligible SSH/password, key-passphrase, or shared HTTPS sign-in prompt. Unlock the vault before choosing to save. **Use saved credential** fills a matching prompt and requires you to continue explicitly. Trust, username, and keyboard-interactive/OTP prompts are excluded. The manager supports locking, changing the master password and deleting entries; locking/deletion clear corresponding vault-filled fields. Only opaque references enter workspace JSON. Native Windows/macOS vault UI/ACL and power-loss validation remain pending. Embedded SSH presents its own authentication prompts and checks user/app host-trust stores. OpenSSH compatibility uses your installed client configuration. Text macro recording captures typed input, so stop recording before entering secrets. Damaged settings files are preserved until an explicit backup/reset action.

Screenshot runs use the isolated `data/screenshot/` directory unless `--data-dir` overrides it.

### Linux and macOS desktop launch integration

Linux release packages include `desktop-integration.py install|status|uninstall`.
Run it with Python 3 from the extracted package. Install copies the packaged
application, adjacent helpers, runtime manifests and penguin icon into
`${XDG_DATA_HOME:-$HOME/.local/share}/termcraft/desktop`, then registers a menu
entry, optional directory “Open With TermCraft” handler, open-here action and
bookmark MIME/URL handlers. The installed copy keeps working after the extracted
archive moves or is deleted. Run install from the new release to upgrade; close
TermCraft first. Workspace settings remain in their existing configuration
location. Removal checks owned-file hashes and refuses to remove user-modified
entries. It preserves unrelated files and user default associations.

macOS packages use a native AppKit launcher for Finder URL/file events and the
Finder Services “TermCraft/Open terminal here” action. It forwards literal
arguments to the adjacent Rust application; the Rust executable and RDP helper
remain beside each other. Bundle-relative paths survive moving the `.app`.
Optional explicit registration, status and removal use Python 3:
`python3 TermCraft.app/Contents/Resources/desktop-integration.py install|status|uninstall`.
Registration records only owned bundle paths, supports registering a moved/new
bundle, and never chooses a default application. Finder Services may need to be
enabled in macOS Keyboard settings. Native event delivery and Services execution
still require validation on a Mac; Linux checks only declarations/source/layout.

Desktop URLs accept only `termcraft://session/<percent-encoded-stable-ID>` or
`termcraft://cwd?path=<percent-encoded-absolute-directory>`. A `.termcraft-link`
file is UTF-8 JSON, at most 16 KiB, containing exactly `{"version":1,"session":"ID"}`
or `{"version":1,"cwd":"/absolute/directory"}`. Missing bookmark IDs produce the
normal workspace launch error. `--open-url` and `--open-file` use the normal
workspace/IPC dispatcher, preserve an explicit `--data-dir`, and reject additional
terminal actions, unknown fields, commands, credentials and workspace overrides.
