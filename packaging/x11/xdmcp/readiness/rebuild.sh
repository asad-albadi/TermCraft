#!/usr/bin/env bash
# Rebuild in an unprivileged namespace with the supplied, locally pinned source.
set -euo pipefail
inputs=$(cd -- "$(dirname -- "$0")" && pwd)
archive=$(realpath -- "${1:?pass the xorg-server-21.1.26.tar.xz archive}")
output=$(realpath -m -- "${2:?pass a new output directory}")
mkdir -- "$output"
python - "$inputs" "$archive" <<'PY'
import hashlib, json, pathlib, sys
d = pathlib.Path(sys.argv[1])
m = json.loads((d.parent / 'manifest.json').read_text())
for file, expected in [(pathlib.Path(sys.argv[2]), m['source_sha256']), (d/'xephyr.patch',m['clipboard_engine']['patch_sha256']), (d/'xtrans-1.6.0-headers.tar.xz',m['clipboard_engine']['xtrans_headers_sha256'])]:
    if hashlib.sha256(file.read_bytes()).hexdigest() != expected:
        raise SystemExit(f'Unexpected build input: {file.name}')
PY
mkdir -p "$output"/{engine-src,build-xephyr,xtrans/include,xtrans/lib/pkgconfig}
tar -xf "$archive" -C "$output/engine-src" --strip-components=1
patch -d "$output/engine-src" -p1 < "$inputs/xephyr.patch"
tar -xf "$inputs/xtrans-1.6.0-headers.tar.xz" -C "$output/xtrans/include"
cp "$inputs/xtrans.pc" "$output/xtrans/lib/pkgconfig/"
run=(bwrap --ro-bind / / --bind "$output" /mnt --tmpfs /tmp --dev /dev --proc /proc --unshare-net --chdir /mnt/build-xephyr --setenv PKG_CONFIG_PATH /mnt/xtrans/lib/pkgconfig --)
"${run[@]}" /mnt/engine-src/configure --enable-kdrive --enable-xephyr --disable-xorg --disable-xvfb --disable-xnest --enable-glamor --enable-glx --enable-dri --enable-dri2 --enable-dri3 --disable-config-udev --disable-devel-docs --disable-docs --disable-selective-werror
"${run[@]}" make -j2
install -m755 "$output/build-xephyr/hw/kdrive/ephyr/Xephyr" "$output/Xephyr-session-ready"
strip --strip-unneeded "$output/Xephyr-session-ready"
sha256sum "$output/Xephyr-session-ready"
