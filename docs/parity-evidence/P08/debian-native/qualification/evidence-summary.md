# Genuine native Debian core qualification

PASS. The genuine locked native release archive was wrapped using the accepted MAIN Python 3.11-compatible scripts and installed through apt in a new Debian 12 container. No application source changed.

## Identity

- Source snapshot: `4726eebadb56a15c2e9ed9f4018a63ffe9a55e9834ad927fea5472f2f2c29b53`; native build provenance is the parent directory's `verified-*` evidence.
- Portable SHA256: `08969e0223e0b74ec954a1440a8f70c1eab03c9ba7d53fe7c8129b29e08b12f4`.
- Debian SHA256: `2bad678090823cf3aeb990ddf785cd364fd81b52fa3eaf823c50120251a6e53f`.
- Debian artifact: `target/validation/p08-debian-native/qualification/out/termcraft_0.1.0_amd64.deb` (33,006,052 bytes).
- Wrapper image: `sha256:1e7cd41f77c63d2093907d8a33cc65562eafd5cfbed60c829b9faedf652f24a0`, Python 3.11.2.
- Fresh runtime image: Debian bookworm `sha256:1d04200c2ece1d4d75a2a6f8e11ebce7d86a2ae28b34bb2c9cbb3ee9f4953e75`.

## Executed checks

1. Independently read and SHA256-verified all 1,074 portable payload files against its Linux x86_64 release manifest. The unchanged production wrapper repeated release/profile/hash and ABI-library allowlist checks.
2. Wrapped successfully under Python 3.11.2; `dpkg-deb --info` and `--contents` inspected package metadata, root ownership, desktop entry and launcher. Declared minimum glibc 2.35, based on actual binaries (GUI 2.35, helper 2.34).
3. New official Debian 12 container had no TermCraft; apt installed the local package and resolved its actual declared dependencies before any test tools were added. `dpkg --audit`, `dpkg -V termcraft`, and both binary `ldd` checks passed.
4. Installed explicit display/testing prerequisites (Xvfb, PIL, fonts), then disconnected container network. `/proc/net` showed loopback only. Tests ran as UID/GID 1000, isolated HOME/workspaces, software-rendered X11 1280x800, empty application PATH, without host display mounts or unsafe browser flags.
5. Unchanged portable startup checker passed GUI screenshot/nonblank checks, helper Ready wire handshake, loopback RDP negotiation, sealed memfd child SHA256 identity and saved session/preferences preservation.
6. Installed `/usr/bin/termcraft` launcher repeated package-file verification, help, native GUI screenshot, helper Ready, loopback RDP negotiation, verified sealed child digest, and workspace preservation. Screenshot visually inspected: complete TermCraft home UI, no failure dialog.
7. Removed package using apt. Launcher and `/opt/termcraft` disappeared. Both user sentinel and real generated workspace JSON hashes were byte-for-byte identical before and after removal; dpkg audit remained clean.
8. Finally removed the owned runtime container; label-filtered owned qualification container inventory was empty. Wrapper container used --rm. No images were removed; no host installation or system changes were made.

## Evidence and scope

Exact wrapper command and raw output: `wrap-command.json`, `wrap.log`. All runtime commands, exit codes and timing: `qualification-commands.json`, individual raw `.log` files. `run-qualification.py` and `installed-smoke.py` preserve the exact test drivers. Input script and desktop hashes: `input-hashes.json`. Manifest and independent integrity result: `portable-manifest.json`, `portable-manifest-validation.json`. Startup screenshots/logs/results: `portable/` and `installed/`. Final identities and scope: `qualification-result.json`.

This is native Debian 12 x86_64 core package qualification for the supplied earlier 13-default-games source snapshot. It does not qualify the latest advanced-options changes, optional Unix/Chrome/pcap payloads, Wayland, other distributions/architectures, a real remote RDP login, or a user desktop session. RDP evidence covers the initial loopback negotiation and owned verified helper child only. Font/display testing tools were installed after the package dependency-resolution check; Xvfb startup assumes an available display stack and fonts, as ordinary desktops provide.
