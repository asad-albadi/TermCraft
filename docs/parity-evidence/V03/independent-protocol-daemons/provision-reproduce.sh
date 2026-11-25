#!/bin/sh
set -eu
useradd -m -s /bin/bash qualify
printf 'qualify:%s\n' "$(cat /evidence/secret)" | chpasswd
mkdir -p /srv/ftp /run/proftpd /run/sshd
chown qualify:qualify /srv/ftp
openssl req -x509 -newkey rsa:2048 -nodes -keyout /evidence/ca.key -out /evidence/ca.pem -days 2 -subj /CN=OwnedQualificationCA >/evidence/cert-generation.log 2>&1
openssl req -newkey rsa:2048 -nodes -keyout /evidence/server.key -out /evidence/server.csr -subj /CN=localhost >>/evidence/cert-generation.log 2>&1
printf 'subjectAltName=IP:127.0.0.1,DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' >/evidence/leaf.ext
openssl x509 -req -in /evidence/server.csr -CA /evidence/ca.pem -CAkey /evidence/ca.key -CAcreateserial -out /evidence/server.pem -days 2 -extfile /evidence/leaf.ext >>/evidence/cert-generation.log 2>&1
chmod 600 /evidence/ca.key
chmod 600 /evidence/server.key
cat >/evidence/inetd.conf <<'EOF'
2323 stream tcp nowait root /usr/sbin/telnetd telnetd -a off
2513 stream tcp nowait root /usr/sbin/in.rlogind in.rlogind -L
2514 stream tcp nowait root /usr/sbin/in.rshd in.rshd -L
EOF
# Owned container only: trust caller identity at its isolated bridge gateway.
printf '127.0.0.1 asad\n' >/home/qualify/.rhosts
chown qualify:qualify /home/qualify/.rhosts
chmod 600 /home/qualify/.rhosts
/usr/sbin/inetd -d /evidence/inetd.conf >/evidence/inetd.log 2>&1 &
printf '%s\n' "$!" >/evidence/inetd.pid
printf '%s\n' "$(cat /evidence/secret)" | tigervncpasswd -f >/evidence/vnc.pass
chmod 600 /evidence/vnc.pass
Xtigervnc :7 -geometry 640x480 -depth 24 -localhost no -rfbport 5907 -SecurityTypes VncAuth -PasswordFile /evidence/vnc.pass -AlwaysShared >/evidence/vnc-server.log 2>&1 &
printf '%s\n' "$!" >/evidence/vnc.pid
sleep 1
DISPLAY=:7 xsetroot -solid '#dd2222'
DISPLAY=:7 xterm -geometry 60x10+15+15 -title OwnedQualification >/evidence/xterm.log 2>&1 &
sleep 1
DISPLAY=:7 xdotool search --name OwnedQualification windowfocus


for port in 2121 2122 2990; do proftpd -c /evidence/proftpd-$port.conf; done
