# Original mechanics and implementation mapping

All product code is independently written; this sheet records observations from decompilation, not source copies.

| Rule | Original source | Implementation |
|---|---|---|
| SSH compression on adds -C and Compression=yes; off uses Compression=no | 0x8d9830, ev_ab63390df398d56baa9027248d0ad3a9d581ec445dca35dbbd986331d776f622 | SSH launch arguments |
| X11 on uses trusted forwarding (-Y); off explicitly disables both ForwardX11 and ForwardX11Trusted | 0x8d9830 | SSH launch arguments |
| Shell choices include Bash, Zsh, cmd, PowerShell, pwsh, WSL and external shells | 0x8d9830 and 0x97e2f4 (ev_9245063662d0cd7bad026a1e4e03fdafad3707595efdf389156a50ff4132c9f8) | Platform default shell or configured executable |
| Serial baud fallback 0x2580 = 9600 | 0x8d9830 | Serial form default |
| Local, remote and dynamic SSH forwarding are separate tunnel modes | 0x8c7f3c and Delphi port-forwarding resources | Tunnel form -L/-R/-D |
| Personal-edition saved tunnel warning when count >4 (five saved tunnels) | 0x91fd40, ev_5769fab5c0da4ea5376fc3b709b28078c6017bcdc45fa2ef9f4e2bd9302ca66a | Five saved tunnels; existing entries remain usable |
| UTF-8 and xterm-256color, scroll-on-key on, scroll-on-output off, scrollback 360000 | entry 0xb521fc, ev_e7c916b5268d86680375d98852151d6e802b3e746449477a4f5d103b102fe1e1, TCP-capture terminal profile specifically | UTF-8 parser, TERM=xterm-256color; lazy scrollback cap. Other original profiles not proven identical |
| SFTP pipeline 32, download block 32 KiB, upload block 64 KiB, buffer 256 KiB | 0x9182d4, ev_402ec996852a791a077a7f7f02b41b2ab86d90347fa391b93ce8f51bf3580729 | SFTP client -R32 -B32768; backend manages remaining internal windows |
| SSH default window 2048 KiB, minimum 2 KiB, packet cap 256 KiB; sizes shifted left 10 | 0x9182d4 | Evidence only: platform OpenSSH controls internal SSH windows; not claimed reproduced |
| OpenSSH 4..12 recognized for transfer adaptation; some router servers avoid SCP fallback | 0x9182d4 and 0x9e0720 | Evidence only: uses platform SFTP; no silent SCP fallback |
| SSH 22, Telnet 23, Rsh/Rlogin 513/514, FTP 21, RDP 3389, VNC 5900, Mosh 60000, terminal keepalive 60 s | Standard interoperability choices; code searched but literal defaults not all recovered. **Estimated** except serial and captured TCP profile Telnet23 | Protocol selection defaults and configurable ports |

## Limits
No original screenshot or runtime walkthrough could be captured on this Linux host: original execution requires approval, which the environment disables. Full original behavior is unverified. The original's embedded Xorg/PuTTY/RDP/VNC/FTP/browser engines, graphical macros, plugins, protocol integrations, games and server suite are not reproduced by simply launching platform clients. The person-choice tool instructed autonomous selection: platform desktop, browser and X-server clients are used where available. This preserves access to those protocols but differs from the original embedded workspace. Graphical macros, plugins and several servers remain missing. ANSI emulator implements common VT operations, not PuTTY's full protocol and rendering surface. Windows and macOS runtime require testing on those operating systems.

## Additional server investigation
HTTP start handler 0x8a67c8 (ev_80251644c7604669fcda1353323f235ea2dec23539dc27879fe55f203c4353f3) configures a listening port and filesystem root; a privilege warning tests ports below 0x401. The rebuild implements read-only HTTP directory listing/downloads with configurable root, bind address and duration. The default localhost:8080 and 3600-second duration are **estimated**. HTTP range requests, original server modes and access controls are not equivalent.

SSH server 0x8a72ac (ev_c22d08fcc3179ed9ce493faf7f7c6c2230b164211ff26a97289ec979b5b457f5) enables forwarding and SFTP and uses authorized_keys; the rebuild invokes a system sshd with a workspace Ed25519 host key, authorized_keys, and internal-sftp. It uses localhost:2222 and key-only authentication as **estimated** cross-platform settings. Iperf start 0x8aac54 (ev_03028c02bf058254fc1dadf6fd45b9ad467edc93eb8ff1c3c218b1a5a37b7649) starts version 2 or 3 with -s; adapters implement both when installed. Cron 0x8aa7a8 and FTP 0x8a6028 were also decompiled but equivalent managed services remain unimplemented.
