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

build_one() {
  local target=$1
  local prepared=$work/source-$target
  local build=$work/build-$target
  python3 "$script_dir/native/prepare-source.py" --platform macos \
    --source-archive "$source_archive" --destination "$prepared"
  meson setup "$build" "$prepared" \
    --buildtype=release --wrap-mode=nodownload \
    -Dxquartz=true -Dxorg=false -Dxephyr=false -Dxnest=false -Dxvfb=false \
    -Dxcsecurity=true -Dbundle-id-prefix=org.termcraft.private \
    -Dsparkle-feed-url= -Dsparkle-public-edkey= \
    -Dc_args="-arch $target -isysroot $sdk" \
    -Dcpp_args="-arch $target -isysroot $sdk" \
    -Dobjc_args="-arch $target -isysroot $sdk"
  meson compile -C "$build"
  local binary="$build/hw/xquartz/mach-startup/X11.bin"
  [[ -f "$binary" ]] || { echo "XQuartz direct-entry binary was not produced for $target." >&2; exit 1; }
  printf '%s\n' "$binary"
}

mkdir -p "$work/binaries"
case "$arch" in
  universal)
    arm_binary=$(build_one arm64 | tail -n 1)
    intel_binary=$(build_one x86_64 | tail -n 1)
    mkdir -p "$work/binaries/universal"
    lipo -create "$arm_binary" "$intel_binary" -output "$work/binaries/universal/X11.bin"
    ;;
  *)
    binary=$(build_one "$arch" | tail -n 1)
    mkdir -p "$work/binaries/$arch"
    cp -- "$binary" "$work/binaries/$arch/X11.bin"
    ;;
esac

mkdir -p "$destination/bin" "$destination/resources"
cp -- "$work/binaries/$arch/X11.bin" "$destination/bin/X11.bin"
chmod 0555 "$destination/bin/X11.bin"

# Copy and relocate the complete non-system Mach-O dependency closure.
declare -A copied
queue=("$destination/bin/X11.bin")
while ((${#queue[@]})); do
  image=${queue[0]}
  queue=("${queue[@]:1}")
  if [[ "$image" == "$destination/bin/X11.bin" ]]; then
    loader_prefix='@loader_path/../resources'
  else
    loader_prefix='@loader_path'
  fi
  while IFS= read -r dependency; do
    case "$dependency" in
      /System/Library/*|/usr/lib/*) continue ;;
      /opt/X11/*) echo 'XQuartz system-prefix dependency detected.' >&2; exit 1 ;;
      /*)
        name=$(basename "$dependency")
        target="$destination/resources/$name"
        if [[ ! -e "$target" ]]; then
          [[ -f "$dependency" ]] || { echo "Missing Mach-O dependency: $dependency" >&2; exit 1; }
          cp -- "$dependency" "$target"
          install_name_tool -id "@loader_path/$name" "$target"
        fi
        install_name_tool -change "$dependency" "$loader_prefix/$name" "$image"
        if [[ -z "${copied[$target]+set}" ]]; then
          copied["$target"]=1
          queue+=("$target")
        fi
        ;;
      *) echo "Unexpected non-absolute Mach-O dependency: $dependency" >&2; exit 1 ;;
    esac
  done < <(otool -L "$image" | tail -n +2 | sed -E 's/^\s+([^ ]+) \(.*/\1/')
done

if otool -L "$destination/bin/X11.bin" "$destination/resources"/* 2>/dev/null | grep -F '/opt/X11/' >/dev/null; then
  echo 'Relocation left an /opt/X11 dependency.' >&2
  exit 1
fi

for image in "$destination/bin/X11.bin" "$destination/resources"/*; do
  [[ -f "$image" ]] || continue
  codesign --force --sign - "$image" >/dev/null
done

python3 - "$destination" "$source_archive" "$sdk" "$arch" <<'PY'
import hashlib, json, os, plistlib, subprocess, sys
from pathlib import Path
destination, source_archive, sdk, arch = map(Path, sys.argv[1:])
def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024), b''): h.update(chunk)
    return h.hexdigest()
binary=destination/'bin/X11.bin'
info={
    'schema': 1,
    'status': 'unqualified private native build; native lifecycle and graphics evidence required',
    'architecture': arch,
    'sdk': str(sdk),
    'source_archive_sha256': digest(source_archive),
    'files': {str(p.relative_to(destination)): digest(p) for p in sorted(destination.rglob('*')) if p.is_file()},
    'executable': {'path': 'bin/X11.bin', 'sha256': digest(binary)},
}
(destination/'NATIVE-BUILD.json').write_text(json.dumps(info, indent=2)+'\n')
PY

echo "Staged unqualified private XQuartz candidate at $destination" >&2
