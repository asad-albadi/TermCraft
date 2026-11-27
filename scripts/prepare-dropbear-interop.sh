#!/bin/sh
# Private endpoint dependency only. Never installs or edits host SSH/account files.
set -eu
interop_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
interop_tools=${TERMCRAFT_DROPBEAR_DIR:-"$interop_root/target/interop-tools/dropbear/dropbear-2026.94"}
# Only fixture-owned checkout storage is eligible for publication. Resolve parent
# symlinks before validation; never remove or rename an arbitrary supplied tree.
interop_parent=$(python3 - "$interop_root" "$interop_tools" <<'PYTHON'
from pathlib import Path
import sys
root, requested = map(Path, sys.argv[1:])
allowed = (root / 'target/interop-tools').resolve()
parent = requested.absolute().parent.resolve()
if requested.name != 'dropbear-2026.94' or not parent.is_relative_to(allowed):
    raise SystemExit('Dropbear destination must be dropbear-2026.94 below this checkout target/interop-tools')
print(parent)
PYTHON
)
interop_tools="$interop_parent/dropbear-2026.94"
mkdir -p "$interop_parent"
interop_owner='TermCraft private Dropbear fixture v1'
if [ -e "$interop_tools" ] || [ -L "$interop_tools" ]; then
    [ -L "$interop_tools" ] && [ -f "$interop_parent/.termcraft-dropbear-owner" ] && [ "$(cat "$interop_parent/.termcraft-dropbear-owner")" = "$interop_owner" ] || { echo 'Refusing to replace unmanaged Dropbear destination' >&2; exit 1; }
    python3 - "$interop_parent" "$interop_tools" <<'PYTHON'
from pathlib import Path
import sys
parent, link = map(Path, sys.argv[1:])
if not link.resolve().is_relative_to(parent / '.stages'):
    raise SystemExit('Refusing to replace a Dropbear symlink outside owned stages')
PYTHON
fi
printf '%s\n' "$interop_owner" > "$interop_parent/.termcraft-dropbear-owner"
interop_stage=
interop_publishlink=
interop_published=no
interop_gpg=$(mktemp -d /tmp/tc-dropbear-gpg.XXXXXXXX)
interop_cleanup() {
    gpgconf --homedir "$interop_gpg" --kill all >/dev/null 2>&1 || true
    rm -rf -- "$interop_gpg"
    if [ -n "$interop_publishlink" ]; then rm -f -- "$interop_publishlink"; fi
    if [ "$interop_published" = no ] && [ -n "$interop_stage" ]; then rm -rf -- "$interop_stage"; fi
}
trap interop_cleanup EXIT HUP INT TERM
interop_url=https://matt.ucc.asn.au/dropbear/releases
for interop_file in dropbear-2026.94.tar.bz2 dropbear-2026.94.tar.bz2.asc dropbear-key-2015.asc; do
    curl -fL --max-time 90 "$interop_url/$interop_file" -o "$interop_parent/$interop_file"
done
printf '%s  %s\n' e098034a843699200c8c977a991fff73159735bf795d5f72ef672c41a6b1ae81 "$interop_parent/dropbear-2026.94.tar.bz2" | sha256sum -c -
gpg --batch --homedir "$interop_gpg" --import "$interop_parent/dropbear-key-2015.asc" > "$interop_parent/signature.log" 2>&1
gpg --batch --homedir "$interop_gpg" --status-fd 1 --verify "$interop_parent/dropbear-2026.94.tar.bz2.asc" "$interop_parent/dropbear-2026.94.tar.bz2" >> "$interop_parent/signature.log" 2>&1
grep -q 'VALIDSIG F7347EF2EE2E07A267628CA944931494F29C6773 ' "$interop_parent/signature.log"
gpgconf --homedir "$interop_gpg" --kill all
# Build only from a new extraction of the archive just verified above. No file
# from a prior source/object tree is executed or reused, even on repeated runs.
mkdir -p "$interop_parent/.stages"
interop_stage=$(mktemp -d "$interop_parent/.stages/verified.XXXXXXXX")
tar -xjf "$interop_parent/dropbear-2026.94.tar.bz2" -C "$interop_stage"
interop_build="$interop_stage/dropbear-2026.94"
interop_sftp=${TERMCRAFT_SFTP_SERVER:-}
if [ -z "$interop_sftp" ]; then
    for interop_candidate in /usr/lib/ssh/sftp-server /usr/lib/openssh/sftp-server /usr/libexec/sftp-server; do
        if [ -x "$interop_candidate" ]; then interop_sftp=$interop_candidate; break; fi
    done
fi
[ -n "$interop_sftp" ] && [ -x "$interop_sftp" ] || { echo 'External OpenSSH SFTP subsystem unavailable' >&2; exit 1; }
# Prevent C-string injection in the one explicit configured subsystem path.
case "$interop_sftp" in *[!a-zA-Z0-9_./+-]*) echo 'Unsupported SFTP subsystem path characters' >&2; exit 1;; esac
printf '#define SFTPSERVER_PATH "%s"\n#define DROPBEAR_SVR_PASSWORD_AUTH 0\n#define DROPBEAR_SVR_PAM_AUTH 0\n' "$interop_sftp" > "$interop_build/localoptions.h"
if [ "$(uname -s)" = Darwin ]; then
    # Darwin lacks setresgid. Switch in the session child and omit Unix socket forwarding.
    printf '#define DROPBEAR_SVR_DROP_PRIVS 0\n#define DROPBEAR_SVR_LOCALSTREAMFWD 0\n#define DROPBEAR_SVR_REMOTESTREAMFWD 0\n' >> "$interop_build/localoptions.h"
fi
cd "$interop_build"
./configure --disable-zlib --disable-syslog --disable-shadow --disable-lastlog --disable-utmp --disable-utmpx --disable-wtmp --disable-wtmpx --disable-loginfunc --disable-pututline --disable-pututxline --enable-bundled-libtom > "$interop_parent/configure.log" 2>&1
make -j2 PROGRAMS='dropbear dropbearkey' > "$interop_parent/build.log" 2>&1
./dropbear -V
sha256sum dropbear dropbearkey "$interop_sftp" > "$interop_parent/binary-sha256.txt"
# Publish after successful fresh configure/build only. os.replace replaces the
# managed symlink itself atomically and never follows it into the previous tree.
interop_publishlink="$interop_parent/.publish.$$"
ln -s "$interop_build" "$interop_publishlink"
python3 - "$interop_publishlink" "$interop_tools" <<'PYTHON'
import os, sys
os.replace(sys.argv[1], sys.argv[2])
PYTHON
interop_published=yes
printf 'Ready: TERMCRAFT_DROPBEAR_DIR=%s\n' "$interop_tools"
