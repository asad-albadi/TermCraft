#!/usr/bin/env bash
set -euo pipefail
if (( $# != 3 )); then
  echo 'Usage: prepare_xdmcp_chooser_fixture.sh BASE_ARCHIVES CHOOSER_ARCHIVES NEW_FIXTURE_DIR' >&2
  exit 2
fi
base=$(realpath -- "$1")
extra=$(realpath -- "$2")
dest=$(realpath -m -- "$3")
support=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ ! -e "$dest" ]]
check() { printf '%s  %s\n' "$1" "$2" | sha256sum --check --status; }
check 40f414912600e2d7c7bda12d4ce8698f8a488e6ea3658bf61d42319e4d8b27e0 "$base/xdm.pkg.tar.zst"
check cf538c4400abd6bdba8942bfdddb0c01716528e75bd95279b4b0a842fa035052 "$base/libxpm.pkg.tar.zst"
check 9494aef0911a031c53670725b5c8c9bb9d3f7c5ea7318b1f72ddd9dcbbeceb6a "$base/xdm-1.1.17.tar.xz"
check 79966ba7df3cf46bbcac4292384247b0070bfd036f3a55dbcd9121cb4ccf8a38 "$extra/libxaw-1.0.16-2-x86_64.pkg.tar.zst"
check 6719bb6abe910d67ef215bba776503f00d6346cbe1f04231f6b4f4be4fad487c "$extra/libxmu-1.3.1-1.1-x86_64_v3.pkg.tar.zst"
extract() {
  bsdtar -xOf "$1" "$2" > "$3"
  test -s "$3"
}
mkdir -p "$dest"/{app-defaults,lib,notices}
extract "$base/xdm.pkg.tar.zst" usr/bin/xdm "$dest/xdm"
extract "$base/xdm.pkg.tar.zst" usr/lib/X11/xdm/chooser "$dest/chooser"
extract "$base/xdm.pkg.tar.zst" usr/lib/X11/xdm/libXdmGreet.so "$dest/libXdmGreet.so"
extract "$base/xdm.pkg.tar.zst" usr/share/X11/app-defaults/Chooser "$dest/app-defaults/Chooser"
extract "$base/xdm-1.1.17.tar.xz" xdm-1.1.17/COPYING "$dest/notices/xdm-COPYING"
extract "$base/libxpm.pkg.tar.zst" usr/lib/libXpm.so.4.11.0 "$dest/lib/libXpm.so.4"
extract "$base/libxpm.pkg.tar.zst" usr/share/licenses/libxpm/COPYING "$dest/notices/libxpm-COPYING"
extract "$extra/libxaw-1.0.16-2-x86_64.pkg.tar.zst" usr/lib/libXaw7.so.7.0.0 "$dest/lib/libXaw.so.7"
extract "$extra/libxaw-1.0.16-2-x86_64.pkg.tar.zst" usr/share/licenses/libxaw/COPYING "$dest/notices/libxaw-COPYING"
extract "$extra/libxmu-1.3.1-1.1-x86_64_v3.pkg.tar.zst" usr/lib/libXmu.so.6.2.0 "$dest/lib/libXmu.so.6"
extract "$extra/libxmu-1.3.1-1.1-x86_64_v3.pkg.tar.zst" usr/share/licenses/libxmu/COPYING "$dest/notices/libxmu-COPYING"
chmod 0755 "$dest/xdm" "$dest/chooser"
cp -- "$support/xdmcp_window.py" "$dest/"
