# TermCraft privacy policy

Last updated: November 25, 2026.

TermCraft is an open-source desktop workspace for local terminals, remote connections and file transfers. This policy describes the application in this repository. Remote hosts, websites, operating-system services, plugins and external programs have their own data handling practices.

## Accounts, advertising and analytics

TermCraft does not operate a TermCraft account service or a hosted workspace service. The application does not include an advertising service, product analytics SDK or built-in crash-report upload service. It does not upload your workspace to a TermCraft server.

## Local workspace data

Sessions, hostnames, usernames, connection settings, tunnels, macros and preferences are stored locally. Existing Harbor configuration locations are retained for compatibility:

| Platform | Default settings directory |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/harbor`, or `~/.config/harbor` |
| macOS | `~/Library/Application Support/Harbor` |
| Windows | `%LOCALAPPDATA%\Harbor`, with the user profile as fallback |

You can choose another directory with `--data-dir`. Browser profiles can contain website cookies and cached data. Remote editing keeps recovery snapshots under `remote-recovery/` in the settings directory; closing or discarding an editor does not necessarily delete those copies. Logs, exported terminal output, screenshots, recordings, transfers and key files may also remain in the locations you choose.

These ordinary workspace files, logs, recordings and recovery snapshots are not all encrypted. Commands, terminal output and recorded keystrokes can contain sensitive information, including secrets typed while recording a macro.

## Credentials, keys and trust

Saving eligible credentials is optional and uses the encrypted credential vault. Passwords are not automatically saved in workspace JSON; saved workspace entries use references to vault records. You control vault creation, unlocking, locking, master-password changes and deletion of saved entries. Credentials necessarily pass through application or selected helper memory while authenticating.

SSH keys and agent identities are used for the connections you configure. Explicitly selected system/OpenSSH compatibility clients can read their own configuration, trust stores and agents. TermCraft maintains connection trust information and presents host-key or certificate decisions where supported. The vault does not encrypt unrelated logs, recovery text, commands, browser data or files exported outside it.

## Network connections and remote access

Remote sessions, transfers, tunnels, shared-session sources, browser pages and diagnostic tools communicate with the endpoints you configure or open. Shared sources and active connections can refresh or reconnect in the background while enabled. The destination can receive your IP address, authentication information required by its protocol, commands, files or other content you send. A configured proxy or SSH gateway also participates in the connection.

TermCraft's embedded browser uses private profiles. Visited websites, the selected browser engine and external clients are separate software or services; their network behavior and privacy policies also apply. Opening documentation, support links or this policy in a browser sends a normal request to the destination site, including GitHub where applicable.

Managed sharing services make the selected files, desktop or other configured resources accessible through their listeners. Their access depends on the protocol, chosen interface and authentication settings. Desktop sharing can expose visible application content; clipboard sharing and remote control expose the corresponding data or input capability when enabled. Supported Wayland sharing uses the desktop portal's permission flow.

## Plugins, commands and external programs

Plugins, macros, shells, startup commands and installed compatibility clients can run with your account's permissions when authorized or invoked. Their own actions can read files, contact networks or send information. Permission to run a plugin is not a promise that it is isolated from your files or network. Inert inspection of a legacy package does not itself register or execute that package.

## Deleting data

Use the application to delete saved credentials, sessions and other supported entries. To remove remaining local data, close TermCraft and remove the relevant settings, recovery, browser-profile or output files. Removing application binaries does not necessarily remove your settings or exported files. Remote copies, backups, shared-source files and operating-system backup services are controlled separately.

## Support and contact

Information you submit through [GitHub issues](https://github.com/asad-albadi/TermCraft/issues) is public and handled by GitHub under its own policies. Do not include passwords, private keys, unredacted terminal logs, private host details or other information you do not want published. For questions about this policy, contact the maintainer through the [GitHub profile](https://github.com/asad-albadi) or open an issue containing only non-sensitive information.

## Changes

This policy may change when the application's data handling changes. The date above and the repository's version history identify updates. Windows/macOS functionality and native qualification remain in progress; this policy does not claim that every described workflow is available on every platform.
