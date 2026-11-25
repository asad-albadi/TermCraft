# Credits

TermCraft is an independently implemented Rust remote workspace. Interface symbols are drawn in code; no artwork, logos, fonts or proprietary program code extracted from the original application is included.

The penguin brand artwork was supplied by the user. `assets/termcraft-logo.png` is the cleaned, tightly framed green rounded-square icon; `assets/termcraft-mascot.png` preserves the supplied standalone portrait.

Dependencies: eframe/egui (MIT OR Apache-2.0), serde and serde_json (MIT OR Apache-2.0), image (MIT OR Apache-2.0), libc (MIT OR Apache-2.0), rfd (MIT), zbus (MIT, Linux appearance portal), windows-sys (MIT OR Apache-2.0), and their transitive dependencies as listed in Cargo.lock. clipboard-win (Boost Software License 1.0) is vendored with its license because the only cached release is marked yanked in the offline package index. OpenSSH and other installed protocol clients are separate programs and retain their own licenses.

The supplied executable is used only as investigation evidence. The original program and product names belong to their owners. No affiliation is claimed.

Interface typography: Liberation Sans (SIL Open Font License 1.1) and DejaVu Sans Mono (Bitstream Vera/DejaVu license); full license texts are included beside the font assets. These are system-font copies, not assets extracted from the original.

Embedded SSH/SFTP dependencies: russh (Apache-2.0), russh-sftp (MIT), tokio (MIT), zeroize (Apache-2.0 OR MIT), hmac, sha1 and sha2 (MIT OR Apache-2.0). Exact versions and transitive packages are recorded in Cargo.lock.

Filesystem/platform safety: rustix, cap-std and cap-fs-ext (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT), getrandom (MIT OR Apache-2.0), subtle (BSD-3-Clause), and tempfile (MIT OR Apache-2.0). Versions and transitive dependencies are pinned by Cargo.lock.


The optional SCP compatibility mode uses the remote host's Python 3 standard library; Python is not bundled here. It implements the single-file SCP data phase independently and adds explicit publication acknowledgements plus a bounded byte-framed folder payload. No installed `scp` executable or tar parser is required. Its independent helper and protocol client use the [OpenSSH portable SCP source](https://github.com/openssh/openssh-portable/blob/V_9_2_P1/scp.c) as a protocol reference. The SCP helper requires a POSIX remote environment; atomic no-replace commits use supported Linux/macOS primitives.

Terminal Unicode/storage: unicode-width 0.2.2, unicode-segmentation 1.13.3 and smol_str 0.2.2 (MIT OR Apache-2.0). htop was used only as an optional external validation program and is not bundled or linked.

Terminal pattern highlighting uses regex 1.12.2 (MIT OR Apache-2.0); exact transitive versions and licenses are recorded in Cargo.lock and their crate metadata.

Terminal legacy character encoding uses encoding_rs 0.8.35 (MIT OR Apache-2.0; additional data/source attributions are retained in the crate COPYRIGHT file).

Endpoint-bound clipboard reads use arboard 3.6.1 (MIT OR Apache-2.0). The optional installed Wayland helper wl-paste belongs to wl-clipboard (GPL-3.0-or-later) and is not bundled by this app.

Session-import format research uses [Mobatek’s CSV documentation](https://blog.mobatek.net/post/mobaxterm-import-csv/), [Ruzgfpegk’s firsthand MobaXterm 26.3 format study](https://gist.github.com/Ruzgfpegk/ab597838e4abbe8de30d7224afd062ea), and setting names/storage behavior from [official PuTTY 0.85 sources](https://the.earth.li/~sgtatham/putty/0.85/putty-0.85.tar.gz). Fixtures are authored and labelled; no native 26.6/Windows export capture is implied.

Shared HTTPS sources use ureq 3.5.2 (MIT OR Apache-2.0), url 2.5.8 (MIT OR Apache-2.0), base64 0.22.1 (MIT OR Apache-2.0), rustls/platform certificate verification, and the exact transitive versions in Cargo.lock.

Local instance dispatch uses interprocess 2.4.9 (0BSD OR Apache-2.0), plus widestring 1.2.1 (MIT OR Apache-2.0) on Windows. Shortcut format references are the Freedesktop Desktop Entry specification and Microsoft Windows Script Host documentation.

The opt-in credential vault uses RustCrypto argon2 0.6.0 and chacha20poly1305 0.11.0 (MIT OR Apache-2.0) with zeroize. Argon2 parameter research uses [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html); the versioned format and application binding logic are independent TermCraft code.

SSH key generation/import and the owned agent use russh/ssh-key and their RustCrypto dependencies; public PPK test vectors from ssh-key 0.7.0-rc.12 retain licenses and provenance under tests/fixtures/keys. Background agent serving uses futures 0.3.32 (MIT OR Apache-2.0) and interprocess Tokio support.

Embedded RDP uses an independently locked headless helper under `engines/rdp/`: IronRDP connector/core/PDU/graphics/session/Tokio crates (MIT OR Apache-2.0), SSPI/Picky, rustls and the platform certificate verifier, plus their dependencies. Exact versions are in `engines/rdp/Cargo.lock`. The separately locked owned protocol fixture/evaluation package preserves upstream example licenses under `docs/parity-evidence/rdp-prototype/`; it is test infrastructure, not a shipped RDP server.

The embedded classic VNC transport is independently written against [RFC 6143](https://www.rfc-editor.org/rfc/rfc6143.html). Classic VNC password challenge authentication uses RustCrypto des 0.9.0 (MIT OR Apache-2.0), with its zeroize feature; this legacy protocol does not encrypt the desktop transport and is explicitly opt-in. It does not include proprietary VNC code or assets.

VNC VeNCrypt X509 transport uses rustls 0.23.46, rustls-platform-verifier 0.7.1 and tokio-rustls 0.26.6 (MIT OR Apache-2.0; transitive notices in Cargo.lock). Protocol extension reference: [RFB protocol community specification](https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst#vencrypt).
