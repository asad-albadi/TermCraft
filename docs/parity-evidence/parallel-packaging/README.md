# TermCraft package verification

The Linux x86-64 portable archive and amd64 Debian package use source snapshot
`0a242a28146920d69d7fabac4c2f4b931aa02c698016c261ee32a2da84043a47`.
The portable manifest records each build input hash and each packaged file
hash. The Debian manifest identifies the portable archive it contains.
[Portable archive](../../../dist/TermCraft-0.1.0-linux-x86_64.tar.gz) ·
[Debian package](../../../dist/termcraft_0.1.0_amd64.deb) ·
[SHA-256 checksums](release.sha256)

Both startup runs opened the relocated workspace with an empty PATH and
produced a 938×990 screenshot. The app launched its adjacent RDP engine and
sent initial negotiation to a loopback fixture. The saved profile and font
preference stayed intact.
[Wayland evidence](startup/verification.json) ·
[X11 evidence](startup-x11/verification.json)

The extracted Debian payload passed 971 file checksums and repeated the RDP
helper path and workspace preservation checks. The host is CachyOS Linux
with glibc 2.44. The app imports glibc symbols through 2.43, and the Debian
control file requires libc6 2.43 or newer.
[Debian verification](debian-verification.log)

Six packaging tests cover reproducible Debian assembly, portable relocation,
desktop entry quoting and ownership, workspace preservation, Windows/macOS
archive layout, and a live edit during a snapshot build. The earlier verified
portable archive and Debian package remain in the checksum-named snapshots directory.
[Staging results](staging-tests.log)

These are unsigned local Linux builds. Clean Debian installation, native
Windows/macOS execution, signing, notarization, and external publication
remain pending. The Windows installer and macOS app inputs were exercised
through synthetic native-format fixtures on Linux. Optional installed
compatibility clients and libpcap are declared in the runtime manifest.
