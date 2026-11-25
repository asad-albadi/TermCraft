set -euo pipefail
umask 022
mkdir -p /tmp/fixture /tmp/wayland /tmp/store /fixture-bin /run/sshd
useradd -u 1000 -m -s /bin/bash runtimefixture
passwd -d runtimefixture >/dev/null
chown -R 1000:1000 /tmp/wayland /tmp/store /tmp/fixture
chmod 700 /tmp/wayland
ssh-keygen -q -t ed25519 -N '' -f /tmp/fixture/host
ssh-keygen -q -t ed25519 -N '' -f /tmp/fixture/client
cp /tmp/fixture/client.pub /tmp/fixture/authorized_keys
printf '[127.0.0.1]:2222 %s\n' "$(cat /tmp/fixture/host.pub)" >/tmp/fixture/known_hosts
cat >/tmp/fixture/ssh_config <<'EOF'
Host *
 BatchMode yes
 StrictHostKeyChecking yes
 GlobalKnownHostsFile /dev/null
 UserKnownHostsFile /tmp/fixture/known_hosts
 IdentitiesOnly yes
EOF
chown -R 1000:1000 /tmp/fixture
cat >/fixture-bin/mosh-server <<'EOF'
#!/bin/sh
export LD_LIBRARY_PATH=/fixture/lib:/opt/termcraft/runtime/linux-x86_64/mosh-deps/lib
exec /fixture/mosh-server "$@"
EOF
chmod 755 /fixture-bin/mosh-server
cat >/tmp/fixture/sshd_config <<'EOF'
Port 2222
ListenAddress 127.0.0.1
HostKey /tmp/fixture/host
PidFile /tmp/fixture/sshd.pid
AuthorizedKeysFile /tmp/fixture/authorized_keys
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
PermitRootLogin no
LogLevel DEBUG
SetEnv PATH=/fixture-bin:/usr/bin:/bin
EOF
# Only this disposable container's client paths are removed; fixture server remains.
rm -f /usr/bin/ssh /usr/bin/perl
rm -rf /usr/lib/perl5 /usr/share/perl5
! test -e /usr/bin/ssh
! test -e /usr/bin/perl
! test -e /usr/lib/perl5
! test -e /usr/share/perl5
printf 'NO_INSTALLED_PERL_OR_SSH_CLIENT\n' >/evidence/mosh-client-absence.txt
python /evidence/mosh-workspace.py
chown -R 1000:1000 /tmp/store
/usr/bin/sshd -D -e -f /tmp/fixture/sshd_config > /evidence/mosh-sshd.log 2>&1 & sp=$!
Xvfb :78 -screen 0 1360x880x24 -nolisten tcp > /evidence/mosh-xvfb.log 2>&1 & xp=$!
trap 'kill "$sp" "$xp" 2>/dev/null || true; wait "$sp" "$xp" 2>/dev/null || true' EXIT
sleep 1
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/env -i HOME=/home/runtimefixture USER=runtimefixture LANG=C.UTF-8 DISPLAY=:78 XDG_RUNTIME_DIR=/tmp/wayland PATH= LD_LIBRARY_PATH=/opt/termcraft/browser/linux-x86_64/libraries /opt/termcraft/harbor-workspace --data-dir /tmp/store >/evidence/mosh-app.log 2>&1 & ap=$!
sleep 3
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/python /evidence/mosh-bootstrap-monitor.py & mp=$!
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/env -i HOME=/home/runtimefixture USER=runtimefixture LANG=C.UTF-8 DISPLAY=:78 XDG_RUNTIME_DIR=/tmp/wayland PATH= LD_LIBRARY_PATH=/opt/termcraft/browser/linux-x86_64/libraries /opt/termcraft/harbor-workspace --data-dir /tmp/store --session abcdef0123456789abcdef0123456789 >/evidence/mosh-ipc.log 2>&1
sleep 7
win=$(DISPLAY=:78 xdotool search --pid "$ap" | head -1)
DISPLAY=:78 xdotool windowfocus --sync "$win"
DISPLAY=:78 xdotool mousemove --sync 650 500 click 1
DISPLAY=:78 xdotool type --clearmodifiers --delay 20 "printf 'CURRENT_MOSH_ENCRYPTED_UDP_PTY_OK\\n' > /evidence/mosh-remote-marker.txt; stty size > /evidence/mosh-remote-initial-size.txt; printf 'CURRENT_MOSH_VISIBLE_UDP_OK\\n'"
DISPLAY=:78 xdotool key Return
for i in $(seq 1 15); do test -e /evidence/mosh-remote-marker.txt && break; sleep 1; done
DISPLAY=:78 import -window root /evidence/mosh-before.png
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/python /evidence/mosh-inspect.py
wait "$mp"
if ! test -e /evidence/mosh-remote-marker.txt; then echo MISSING_REMOTE_MARKER; cat /evidence/mosh-app.log; exit 1; fi
win=$(DISPLAY=:78 xdotool search --pid "$ap" | head -1)
DISPLAY=:78 xdotool windowsize "$win" 1100 720
sleep 2
DISPLAY=:78 xdotool mousemove --sync 650 470 click 1
DISPLAY=:78 xdotool type --clearmodifiers --delay 20 "stty size > /evidence/mosh-remote-resized-size.txt; printf 'CURRENT_MOSH_TYPED_UDP_OK\\n' > /evidence/mosh-typed-marker.txt"
DISPLAY=:78 xdotool key Return
sleep 2
DISPLAY=:78 import -window root /evidence/mosh-resized.png
cat /evidence/mosh-remote-marker.txt
cat /evidence/mosh-typed-marker.txt
cat /evidence/mosh-remote-initial-size.txt /evidence/mosh-remote-resized-size.txt
kill "$ap"; wait "$ap" || true
sleep 3
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/python /evidence/mosh-cleanup.py
