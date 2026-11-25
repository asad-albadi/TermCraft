#!/bin/bash
set -euo pipefail
work=$(cd "$(dirname "$0")" && pwd)
exec timeout -k 5 100 bwrap --unshare-all --die-with-parent --new-session --clearenv \
 --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib /lib64 --symlink usr/bin /bin \
 --dir /etc --ro-bind "$work/passwd" /etc/passwd --ro-bind "$work/group" /etc/group --ro-bind /etc/fonts /etc/fonts --ro-bind /etc/ld.so.cache /etc/ld.so.cache \
 --dev /dev --proc /proc --tmpfs /tmp --dir /tmp/.X11-unix --dir /tmp/wayland \
 --dir /home --bind "$work/home" /home/test --bind "$work" /work \
 --dir /reference --ro-bind /home/asad/Downloads/MobaXterm/MobaXterm_Personal_26.6.exe /reference/MobaXterm.exe \
 --setenv HOME /home/test --setenv USER test --setenv PATH /work/root/usr/bin:/usr/bin \
 --setenv WINEPREFIX /home/test/prefix --setenv WINEDLLOVERRIDES 'winemenubuilder.exe=d;mscoree=d;mshtml=d' \
 --setenv WINEDEBUG '+timestamp,+err' --setenv DISPLAY :0 --setenv XDG_RUNTIME_DIR /tmp/wayland \
 --setenv WLR_BACKENDS headless --setenv WLR_HEADLESS_OUTPUTS 1 --setenv WLR_RENDERER pixman \
 -- /usr/bin/cage -- /usr/bin/python -I /work/probe.py
