# Current genuine release and normal GUI TigerVNC clipboard proof

**PASS:37 fresh checks.** Genuine current-source archive produced successfully; normal app startup/sealed helper and actual normal-GUI VNC bidirectional clipboard exchange passed. This is configured isolated Linux runtime qualification, not physical-desktop/other-OS parity or a global all-clear.

## Genuine producer and identity

Ordinary native scripts/package-release.py, accepted frozen source **32e512ae85fe16dd44d9b775060a0bd1ac70de345ddcf862a221e0f861db74ad**. The only source-input delta versus d1a54 is **src/desktop/vnc_clipboard.rs**;595 others unchanged. Normal app/RDP and normal optional Wayland helper were built from that same snapshot with verified SDK1.96.1, locked/offline Cargo, isolated Cargo home, CARGO_BUILD_JOBS=2, actual serial --jobs1 wrapper/CPU0 and warm distinct release targets. No validation-cache write or MAIN source edit. Helper build4.34s, RDP49.05s; main11m52s under sharedCPU0, full producer/repackage819.59s. Both build stages exited0; source before/after identical.

- Archive **d8cec853b37018e55c76a837340c86d8cdb406ef4bff9c6008d7ddebd92098f0**: target/package-post-tigervnc-clipboard-fix-reviewed/TermCraft-0.1.0-linux-x86_64-arch-xwayland-xdmcp-mosh-unix-chromium-wayland-sharing.tar.gz.
- Actual app **920f59d2a1486c2c20ccac4aea60df381b79ac2414fb11cf42af6d7d84631e2c**.
- RDP helper unchanged **85f33c8c7987cf84d73748c2d33283cf9878426febcbfed72a3f26acb42beba6**.
- Corrected Wayland helper freshly rebuilt but bit-identical **fa7147099cda83486a4bdc3a59510cecc0e560e10200f4c1bac4bf6ff5b19f59**; no false current-source compilation claim or relabeled old archive.
- Strict transactional extraction and **all3372** manifest file hashes verified. Full commands/env/source inventory/SDK/ELF/helper provenance and producer exit/hashes are in the parent release evidence folder.

## Current normal app: actual execution

Launched only the exact strictly extracted current package, not an API/debug harness. Before VNC, existing ordinary package startup checks passed --help and native helper Ready(version1), then launched a normal app RDP session to an owned startup listener. It sent real TPKT, and the actual child executable was verified as sealed memfd with required seals and exact packaged85f33hash. Saved startup workspace/profile remained preserved. This is a startup/helper-boundary check, not a claim of a complete new RDP interoperability run.

The actual normal GUI was then launched through standard --data-dir/--session using a **nonsecret configured saved VNC profile** for owned127.0.0.1:5907, embedded classic password, explicit trusted unencrypted transport and bidirectional clipboard. This does not claim those session settings were clicked through the configuration GUI. Password authentication, visible remote canvas and clipboard controls/paste were operated in the real GUI via native input in the owned compositor.

- Real independent TigerVNC password authentication succeeded. Actual GUI showed Connected·VNC and the640×480 red/xterm desktop; known rendered pixel(221,34,34), over117000 red pixels. No embedded API harness, app instrumentation or mock server.
- After connection, independent TigerVNC xclip set **SERVER_CLIPBOARD_OWNED_GUI**. GUI stayed Connected with Copy remote clipboard enabled. Clicked that real GUI control; a separately focused private GTK clipboard peer read the **exact server marker** from the private native compositor clipboard. This exercises actual server extended clipboard→production decoder→normal GUI→native clipboard.
- GTK peer set **CLIENT_CLIPBOARD_OWNED_GUI**. Returned to the actual VNC canvas and pressed native Ctrl+V. The independent TigerVNC server's own xclip query returned the **exact client marker**, exercising the normal GUI paste and lazy extended-clipboard request/response path.
- Final GUI still visibly Connected·VNC at143seconds after both exchanges, with clipboard channel ready and remote framebuffer intact.
- Generated daemon password was read from the existing mode0600 fixture input only into volatile controller memory and typed into masked actual authentication UI. It was never placed in commands, app argv/environment or a saved profile. Private-home/config/data/workspace scans reached125files with zero plaintext password hits. Known clipboard markers are nonsecret owned test data.

Actual artifacts: gui-reports/normal-startup/result.json, normal-vnc-authentication.png, actual-vnc-connected.png, actual-copy-remote-clipboard.png, actual-server-to-gui-clipboard-read.png, peer/read-result.json, independent-server-client-clipboard.txt, final-connected-state.png and all native command acknowledgements.

## Scope, cleanup and unchanged server coverage

NormalUID1000 Arch client container, capsdropped/read-only root/privateTMP/HOME/display/PID/dev/clipboard, CPU1. The sole network namespace was the independent daemon's disconnected owned network, no published ports or host network. Read-only host/usr/fonts supplied matching native runtime ABI; exact image/loader/libc hashes and mounts are recorded. No host home/display/input/clipboard was used. Actual headless Mutter software rendering/native PipeWire monitor observer supplied screenshots and fixture input. Initial container NSS lookup forUID1000 failed; generated container-only passwd/group records corrected this fixture setup without host or product changes. Expected unused portal-settings activation warnings are retained; actual normal GUI/clipboard succeeded.

All explicitly owned GUI processes were reaped; GUI container exited0. Both owned GUI/daemon containers are now absent; no termcraft-independent* container remains. Only the four explicitly handed-over generated private inputs secret,ca.key,server.key,vnc.pass were deleted from the daemon scratch directory. Cleanup report retained. Public certificates/nonsecret reproduction guides remain.

The prior125-check genuine packaged server/portal workflow is **not replayed**. SERVER-COVERAGE-TRANSFER.json compares all13 recorded server/GUI/provider source inputs against this current package and confirms exact equality plus helperfa714 identity. Root explicitly authorized this transfer because the sole delta is the unrelated VNC-client clipboard file. Original executed server proof remains bound to8a9 and its report; this report does not mislabel it as newly executed on d8cec.

No new concrete production failure was found. Native client clipboard fix now has actual normal-GUI independent-daemon proof in addition to prior production-API/regression evidence. Normal GUI server/portal proof transfer remains explicit and separately scoped.
