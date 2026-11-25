<p align="center">
  <img src="assets/termcraft-logo.png" alt="TermCraft penguin on an emerald rounded square" width="128">
</p>

<h1 align="center">TermCraft</h1>

<p align="center">
  <b>Local terminals, remote sessions and file transfers, in one desktop workspace.</b><br>
  Organize connections, work across terminal tabs and keep everyday tools close.<br>
  Open source, local, and yours.
</p>

<p align="center">
  <img alt="Built with Rust" src="https://img.shields.io/badge/built_with-Rust-b7410e?style=flat-square&amp;logo=rust">
  <img alt="Platform: Linux" src="https://img.shields.io/badge/platform-Linux-00a884?style=flat-square&amp;logo=linux&amp;logoColor=white">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-3a3a3a?style=flat-square"></a>
  <img alt="Status: early alpha" src="https://img.shields.io/badge/status-early%20alpha-d69e2e?style=flat-square">
</p>

<p align="center">
  <b>Originally created with <a href="https://github.com/storytold/craft-anything">CraftAnything</a> by <a href="https://getartcraft.com/">ArtCraft</a>.</b><br>
  Independently developed since. Inspired by MobaXterm's organization and workflows.
</p>

<p align="center">
  <a href="#get-started"><b>Build and run</b></a> ·
  <a href="https://github.com/asad-albadi/TermCraft/releases">Releases</a> ·
  <a href="https://github.com/asad-albadi/TermCraft/issues">Issues</a> ·
  <a href="https://github.com/storytold/craft-anything">Craft Anything</a> ·
  <a href="https://getartcraft.com/">ArtCraft</a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="shots/termcraft-dark.png">
    <img src="shots/now.png" alt="TermCraft's penguin-branded desktop workspace with emerald accents, a session sidebar, quick connect and terminal tabs" width="100%">
  </picture>
  <br>
  <sub>TermCraft in light and dark appearances. System mode follows the desktop preference.</sub>
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#get-started">Get started</a> ·
  <a href="docs/USAGE.md">User guide</a> ·
  <a href="#settings-and-storage">Storage</a> ·
  <a href="#development">Development</a> ·
  <a href="#status-and-roadmap">Status</a> ·
  <a href="#license-and-credits">License and credits</a> ·
  <a href="PRIVACY.md">Privacy policy</a>
</p>

## Features

<table>
  <tr>
    <td width="33%" valign="top">
      <h3>Work in terminals</h3>
      Local shells and SSH, terminal search and logging, configurable highlighting, split layouts, detached tabs and coordinated input.
    </td>
    <td width="33%" valign="top">
      <h3>Keep connections organized</h3>
      Saved sessions and folders, shared definitions, session imports, SSH gateways, an encrypted credential vault and key/agent management.
    </td>
    <td width="33%" valign="top">
      <h3>Keep tools nearby</h3>
      Graphical remote files and editing, embedded desktops and browser tabs, tunnels, recorded macros, diagnostics and managed sharing services.
    </td>
  </tr>
</table>

- **Remote files:** embedded SSH/SFTP with shared shell/transfer authentication, file and folder transfers, directory following, remote editing and recovery. Explicit OpenSSH and SCP compatibility choices remain available.
- **Remote desktops:** embedded RDP and VNC, directional clipboard controls, resizing and fullscreen. SSH gateways preserve separate gateway and target trust decisions. Device and platform coverage remains partial.
- **Linux X11:** an optional pinned, app-owned X server, per-session SSH forwarding, XDMCP Direct/Indirect discovery, installed keyboard layouts and optional text clipboard sharing.
- **Protocols and tools:** FTP/FTPS, Telnet, Rlogin/Rsh, configurable serial transport, comparison/image/network tools, portable Unix packages and an explicit TermCraft plugin workflow. Legacy package inspection is inert; it does not execute original plugins.
- **Browser and services:** embedded Chromium with private profiles; managed FTP, TFTP, Telnet, HTTP, SSH/SFTP, Iperf, Cron, NFS and selected-display VNC sharing within documented scopes.
- **Workspace extras:** timed terminal/graphical macros, thirteen games with 51 stock puzzle presets and Save/Load, plus an optional idle screen that preserves running sessions.
- **Appearance:** penguin branding, emerald palettes, System/Light/Dark modes and configurable terminal profiles.

Implementation and platform limits are recorded in the [feature matrix](FEATURE_PARITY.md). The [user guide](docs/USAGE.md) explains session setup, transfers, clipboard controls, recovery, keys, storage and desktop integration.

## Get started

### Platform status

| Platform | Current status |
| --- | --- |
| Linux x86-64 | Strongest implementation and validation coverage. Native Debian 12 core packages and a separate Arch-compatible optional-runtime bundle are qualified within their declared scopes. |
| Windows x86-64 | The native GUI and adjacent RDP helper have a verified unsigned portable package and per-user installer on a supported desktop. Clean-user graphics and activation, optional runtimes, native engine release approval and broader device and workflow qualification remain open. |
| macOS | Reviewed source and bundle foundations exist. Native engines, providers, Intel/Apple Silicon delivery and interactive qualification remain unfinished. |

The full optional Linux bundle and Debian core package have different contents and ABI requirements. The Arch-compatible optional bundle requires its declared runtime, including glibc 2.44; it is not a general Debian package. Debian core omits the optional X/browser/Mosh/Wayland runtime payloads. See [packaging documentation](packaging/README.md) and [current qualification](docs/parity-evidence/execution/2026-11-25-local-completion.md).

### Build from source

Clone this repository; its root contains `Cargo.toml` and the build scripts:

```sh
git clone https://github.com/asad-albadi/TermCraft.git
cd TermCraft
rustup toolchain install 1.96.1 --profile minimal --component rustfmt
RUSTUP_TOOLCHAIN=1.96.1 ./scripts/run-dev.sh
```

Rust 1.96.1 is the verified Linux toolchain. Release packaging tools require Python 3.11 or newer. Builds need a native C/C++ toolchain and the platform development libraries used by eframe/egui and the selected providers. A graphical desktop and working OpenGL drivers are required. Tests also need their documented endpoint fixtures; some Linux tests require OpenSSH tools and a C compiler. See [development notes](DESIGN.md) and the [validation evidence](PARITY_PROGRESS.md).

The development script builds both the GUI and its independently locked RDP helper. After dependencies have been downloaded, set `TERMCRAFT_OFFLINE=1` to use Cargo's cache.

For a native release build:

```sh
RUSTUP_TOOLCHAIN=1.96.1 ./scripts/build-release.sh
./target/release/harbor-workspace
```

The internal executable name remains `harbor-workspace` for compatibility. Keep `termcraft-rdp-engine` beside it when copying a release build; prefer a complete package so manifests, assets and any optional runtimes stay together. Windows equivalents use `.exe`. Native Windows/macOS instructions and delivery still require their platform work.

### Packages and GitHub Actions

The repository includes [CI](.github/workflows/ci.yml) and a manually dispatched [release-artifact workflow](.github/workflows/release.yml). These definitions are a foundation, not a claim that every native job or release format is qualified. The manual workflow builds candidate unsigned artifacts; it does not currently provide PixelCraft-style coordinated, tag-triggered GitHub Release publishing.

Check the [Releases page](https://github.com/asad-albadi/TermCraft/releases) for assets actually published. Desktop release automation, platform installers and verification are tracked in the [roadmap](#status-and-roadmap). Availability must follow successful platform checks; planned formats are not advertised as downloadable builds.

### Your first session

1. Choose **Start local terminal** to open your shell.
2. Choose **Create a new remote session**, select a protocol and save its settings.
3. Double-click a saved session to connect. New embedded SSH sessions open their remote file pane automatically.
4. Configure splits, terminal appearance and clipboard behavior in Settings.

See the [user guide](docs/USAGE.md) for recovery after reconnects, shared session sources, shortcuts, SSH keys/agents and desktop launch integration.

## Settings and storage

Sessions, tunnels, macros and preferences are saved locally. Earlier Harbor configuration paths are retained:

| Platform | Default settings directory |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/harbor`, or `~/.config/harbor` |
| macOS | `~/Library/Application Support/Harbor` |
| Windows | `%LOCALAPPDATA%\Harbor`, with the user profile as fallback |

Use `--data-dir /path/to/workspace` to choose an independent workspace. Eligible credentials can be saved through the optional encrypted vault; ordinary logs, recordings and remote-editor recovery snapshots are not all encrypted. Recovery copies can remain after an editor closes. Browser pages, remote connections, sharing services, plugins and external programs have their own data-access implications.

Read the [privacy policy](PRIVACY.md) for network activity, credentials, retained files and deletion details.

## Development

```sh
CARGO_BUILD_JOBS=2 cargo +1.96.1 fmt --all -- --check
CARGO_BUILD_JOBS=2 cargo +1.96.1 test --locked --all-targets --no-fail-fast
CARGO_BUILD_JOBS=2 cargo +1.96.1 test --locked --manifest-path engines/rdp/Cargo.toml
```

The optional Wayland sharing helper has its own manifest and native requirements; see [its build notes](packaging/wayland/README.md). Integration/native runners must be provisioned with their declared fixtures. A skipped or unavailable native check is not evidence of platform support.

| Path | Purpose |
| --- | --- |
| `src/` | Terminal, protocol, session, service and desktop application code |
| `engines/rdp/`, `engines/wayland/` | Independently built native helpers |
| `assets/`, `shots/` | Penguin branding, fonts and workspace screenshots |
| `packaging/`, `scripts/` | Runtime inventories, packaging and validation tools |
| `tests/` | Regression and integration tests |
| `docs/` | Usage, plans and retained qualification evidence |

Read the [design notes](DESIGN.md), [mechanics mapping](MECHANICS.md) and [complete implementation plan](docs/superpowers/plans/2026-11-23-complete-parity-plan.md) for deeper context.

## Status and roadmap

**TermCraft is early alpha. Full MobaXterm Personal 26.6 parity has not been achieved.** The latest recorded implementation checkpoint is **46/60 (76.7%)**. This counts implemented checkpoints, not fully verified cross-platform equivalence. Strict native/original acceptance remains **0/60**.

The latest recorded Linux regression baseline passed **986 tests, zero failures, 73 ignored**. Current qualified Linux artifacts include actual packaged-app VNC clipboard checks and Debian install/upgrade preservation. The [qualification report](docs/parity-evidence/execution/2026-11-25-local-completion.md) distinguishes current GUI checks, retained unchanged-component evidence and unavailable environments. Documentation publication does not imply a new binary qualification run.

Remaining work includes native Windows/macOS engine and provider implementation, browser/runtime packaging, installers, original Personal 26.6 measurements and physical-device/graphics checks. Some POSIX file/serial operations remain explicitly unsupported because equivalent safe APIs are unavailable; another Linux machine or added hardware does not resolve that architecture gap.

The first hosted [GitHub CI run](https://github.com/asad-albadi/TermCraft/actions/runs/45636600121) found an RDP helper allocation-cleanup test failure (81 passed, 1 failed). The root cause is not yet established; GitHub CI is not green. This is tracked in the CI/release issue below, separately from the recorded local baseline.

- [Windows native implementation, packaging and qualification — #1](https://github.com/asad-albadi/TermCraft/issues/1)
- [macOS Apple Silicon and Intel implementation and delivery — #2](https://github.com/asad-albadi/TermCraft/issues/2)
- [Desktop CI and GitHub releases, modeled after PixelCraft without Android — #3](https://github.com/asad-albadi/TermCraft/issues/3)

The [parity checklist](PARITY_CHECKLIST.md), [durable queue](PARITY_EXECUTION_QUEUE.json) and [progress log](PARITY_PROGRESS.md) retain the detailed implementation and evidence records.

## License and credits

TermCraft's original project code is available under the [MIT License](LICENSE). Fonts, vendored code, dependencies, test/reference material and optional runtime payloads retain their own licenses and notices; the project license does not relicense those components. See [CREDITS.md](CREDITS.md) and the component license files before redistributing a bundled runtime.

The penguin artwork was supplied for TermCraft. The interface uses Rust and eframe/egui. TermCraft is independently implemented and is not affiliated with or endorsed by MobaXterm or its owners. Product names belong to their respective owners.

[Privacy policy](PRIVACY.md) · [Report an issue](https://github.com/asad-albadi/TermCraft/issues)
