#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: build-private-macos.sh --source-archive ARCHIVE --destination DIRECTORY [--arch arm64|x86_64|universal]" >&2
  exit 2
}

source_archive=''
destination=''
arch='universal'
while (($#)); do
  case "$1" in
    --source-archive) (($# >= 2)) || usage; source_archive=$2; shift 2 ;;
    --destination) (($# >= 2)) || usage; destination=$2; shift 2 ;;
    --arch) (($# >= 2)) || usage; arch=$2; shift 2 ;;
    *) usage ;;
  esac
done

[[ $(uname -s) == Darwin ]] || { echo 'Private XQuartz builds require a native macOS host.' >&2; exit 2; }
[[ -n "$source_archive" && -f "$source_archive" ]] || { echo 'An explicit pinned xorg-server source archive is required.' >&2; exit 2; }
[[ -n "$destination" && ! -e "$destination" && ! -L "$destination" ]] || { echo 'Destination must be new.' >&2; exit 2; }
case "$arch" in arm64|x86_64|universal) ;; *) echo 'Unsupported architecture.' >&2; exit 2 ;; esac

for tool in meson ninja pkg-config bison flex mig xcrun otool install_name_tool codesign lipo; do
  command -v "$tool" >/dev/null || { echo "Missing native build tool: $tool" >&2; exit 1; }
done

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
root=$(cd -- "$script_dir/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/termcraft-private-xquartz.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
sdk=$(xcrun --sdk macosx --show-sdk-path)
quartz_headers=$(pkg-config --cflags xfont2 x11 gl)

build_one() {
  local target=$1
  local prepared=$work/source-$target
  local build=$work/build-$target
  python3 "$script_dir/native/prepare-source.py" --platform macos \
    --source-archive "$source_archive" --destination "$prepared" >&2 || return
  meson setup "$build" "$prepared" \
    --buildtype=release --wrap-mode=nodownload --prefix "$work/prefix-$target" \
    -Dxquartz=true -Dxorg=false -Dxephyr=false -Dxnest=false -Dxvfb=false \
    -Dxcsecurity=true -Dsecure-rpc=false -Dbundle-id-prefix=org.termcraft.private \
    -Dsparkle-feed-url= -Dsparkle-public-edkey= \
    -Dc_args="-arch $target -isysroot $sdk -DTERMCRAFT_PRIVATE_XQUARTZ $quartz_headers" \
    -Dcpp_args="-arch $target -isysroot $sdk" \
    -Dobjc_args="-arch $target -isysroot $sdk" \
    -Dc_link_args="-arch $target -Wl,-headerpad_max_install_names" \
    -Dcpp_link_args="-arch $target -Wl,-headerpad_max_install_names" \
    -Dobjc_link_args="-arch $target -Wl,-headerpad_max_install_names" >&2 || return
  meson compile -C "$build" >&2 || return
  local binary="$build/hw/xquartz/mach-startup/X11.bin"
  [[ -f "$binary" ]] || { echo "XQuartz direct-entry binary was not produced for $target." >&2; exit 1; }
  printf '%s\n' "$binary"
}

mkdir -p "$work/binaries"
case "$arch" in
  universal)
    arm_binary=$(build_one arm64)
    intel_binary=$(build_one x86_64)
    mkdir -p "$work/binaries/universal"
    lipo -create "$arm_binary" "$intel_binary" -output "$work/binaries/universal/X11.bin"
    ;;
  *)
    binary=$(build_one "$arch")
    mkdir -p "$work/binaries/$arch"
    cp -- "$binary" "$work/binaries/$arch/X11.bin"
    ;;
esac

dependency_prefix=${TERMCRAFT_XQUARTZ_DEPS:?Set TERMCRAFT_XQUARTZ_DEPS to the private dependency prefix}
resource_arch=$arch
[[ "$arch" != universal ]] || resource_arch=arm64
bundle="$destination/XQuartz.app/Contents"
runtime="$destination/opt/X11"
mkdir -p "$bundle/MacOS" "$bundle/Resources" "$runtime/bin" "$runtime/lib" "$runtime/share/X11" "$runtime/share/fonts"
cp -- "$work/binaries/$arch/X11.bin" "$bundle/MacOS/X11.bin"
cp -- "$work/build-$resource_arch/hw/xquartz/bundle/Info.plist" "$bundle/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable X11.bin' "$bundle/Info.plist"
cp -R -- "$work/source-$resource_arch/hw/xquartz/bundle/Resources/." "$bundle/Resources/"
cp -- "$work/source-$resource_arch/hw/xquartz/defaults.plist" "$bundle/Resources/defaults.plist"
mkdir -p "$destination/CORRESPONDING-SOURCE"
cp -- "$source_archive" "$destination/CORRESPONDING-SOURCE/xorg-server.tar.gz"
cp -- "$script_dir/native/xquartz-private-entry.patch" "$destination/CORRESPONDING-SOURCE/"
cp -- "$work/source-$resource_arch/COPYING" "$destination/CORRESPONDING-SOURCE/xorg-server-COPYING"
if [[ -d "$dependency_prefix/share/termcraft/CORRESPONDING-SOURCE" ]]; then
  cp -R -- "$dependency_prefix/share/termcraft/CORRESPONDING-SOURCE/." "$destination/CORRESPONDING-SOURCE/"
fi
cp -- "$dependency_prefix/bin/xkbcomp" "$runtime/bin/xkbcomp"
cp -R -- "$dependency_prefix/share/X11/xkb" "$runtime/share/X11/"
cp -R -- "$dependency_prefix/share/fonts/X11" "$runtime/share/fonts/"
test -s "$runtime/share/fonts/X11/misc/fonts.dir"
python3 "$script_dir/native/relocate-macos.py" "$destination" "$arch"
codesign --force --sign - "$destination/XQuartz.app"
codesign --verify --strict "$destination/XQuartz.app"

python3 - "$destination" "$source_archive" "$sdk" "$arch" <<'PY'
import hashlib, json, os, subprocess, sys
from pathlib import Path
destination, source_archive, sdk = map(Path, sys.argv[1:4])
arch = sys.argv[4]
def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024), b''): h.update(chunk)
    return h.hexdigest()
binary=destination/'XQuartz.app/Contents/MacOS/X11.bin'
info={
    'schema': 1,
    'status': 'unqualified private native build; native lifecycle and graphics evidence required',
    'architecture': arch,
    'deployment_target': os.environ.get('MACOSX_DEPLOYMENT_TARGET', 'toolchain default'),
    'sdk': str(sdk),
    'sdk_version': subprocess.check_output(['xcrun', '--sdk', 'macosx', '--show-sdk-version'], text=True).strip(),
    'compiler': subprocess.check_output(['xcrun', 'clang', '--version'], text=True).splitlines()[0],
    'source_archive_sha256': digest(source_archive),
    'files': {str(p.relative_to(destination)): digest(p) for p in sorted(destination.rglob('*')) if p.is_file()},
    'executable': {'path': binary.relative_to(destination).as_posix(), 'sha256': digest(binary)},
}
(destination/'NATIVE-BUILD.json').write_text(json.dumps(info, indent=2)+'\n')
PY

echo "Staged unqualified private XQuartz candidate at $destination" >&2
