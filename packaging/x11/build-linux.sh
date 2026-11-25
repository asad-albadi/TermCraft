#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
  echo 'Usage: build-linux.sh INSTALLATION_ROOT [PINNED_ARCH_PACKAGE]' >&2
  exit 2
fi
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
  echo 'The qualified bundle targets Linux x86_64 (Arch Linux ABI).' >&2
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
install_root=$1
target="$install_root/x11/linux-x86_64"
mkdir -p -- "$install_root/x11"
if [[ -e "$target" || -L "$target" ]]; then
  echo "Bundle destination already exists: $target" >&2
  exit 2
fi
work=$(mktemp -d -- "$install_root/x11/.xwayland.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
artifact_url='https://archive.archlinux.org/packages/x/xorg-xwayland/xorg-xwayland-24.1.15-1-x86_64.pkg.tar.zst'
archive="$work/package.tar.zst"
if (( $# == 2 )); then
  cp -- "$2" "$archive"
else
  curl --fail --silent --show-error --location --proto '=https' --max-time 90 \
    --output "$archive" "$artifact_url"
fi
(cd -- "$work" && sha256sum --check --status <<'SUM'
0a8be13f25f64f22ed110f4890e907cfceb34009aeab5b0fd316b6a92ab15a7f  package.tar.zst
SUM
) || { echo 'Pinned Arch Xwayland archive checksum does not match.' >&2; exit 1; }

mkdir -- "$work/payload"
tar -xOf "$archive" usr/bin/Xwayland > "$work/payload/Xwayland"
(cd -- "$work/payload" && sha256sum --check --status <<'SUM'
cbedd90fbc960d4cc799792cedf89b0afa1f1f74a93ba9fb05bfd4da2f27b2b8  Xwayland
SUM
) || { echo 'Pinned Xwayland executable checksum does not match.' >&2; exit 1; }
tar -xOf "$archive" usr/share/licenses/xorg-xwayland/COPYING > "$work/payload/COPYING"
tar -xOf "$archive" .BUILDINFO > "$work/payload/BUILDINFO"
tar -xOf "$archive" .PKGINFO > "$work/payload/PKGINFO"
cp -- "$script_dir/manifest.json" "$work/payload/manifest.json"
chmod 0755 -- "$work/payload" "$work/payload/Xwayland"
chmod 0644 -- "$work/payload/"{COPYING,BUILDINFO,PKGINFO,manifest.json}
mv -T --no-clobber -- "$work/payload" "$target"
test ! -d "$work/payload" || { echo "Bundle destination already exists: $target" >&2; exit 2; }
printf 'Staged verified Xwayland payload at %s\n' "$target"
