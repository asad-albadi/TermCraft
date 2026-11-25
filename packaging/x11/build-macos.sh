#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
  echo 'Usage: build-macos.sh CANDIDATE_DIRECTORY [PINNED_XQUARTZ_PKG]' >&2
  exit 2
fi
if [[ $(uname -s) != Darwin ]]; then
  echo 'Stage the XQuartz candidate on macOS.' >&2
  exit 2
fi
candidate=$1
if [[ -e "$candidate" || -L "$candidate" ]]; then
  echo 'Candidate destination already exists.' >&2
  exit 2
fi
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/termcraft-xquartz.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
archive="$work/upstream.pkg"
if (( $# == 2 )); then
  cp -- "$2" "$archive"
else
  curl --fail --silent --show-error --location --proto '=https' --max-time 180 \
    --output "$archive" \
    'https://github.com/XQuartz/XQuartz/releases/download/XQuartz-2.8.6/XQuartz-2.8.6.pkg'
fi
(cd -- "$work" && shasum -a 256 --check --status <<'SUM'
9ac35a505095bfbd3009c3b4772f0c6421e2f79c4210ab908459270d1c447909  upstream.pkg
SUM
) || { echo 'XQuartz package checksum mismatch.' >&2; exit 1; }
# Extract for relocation review without invoking any installation scripts.
pkgutil --expand-full "$archive" "$work/candidate"
engine=$(find "$work/candidate" -type f -name Xquartz -print -quit)
if [[ -z "$engine" ]]; then
  echo 'The pinned package did not contain the Xquartz engine.' >&2
  exit 1
fi
otool -L "$engine" > "$work/candidate/Xquartz-dylibs.txt"
cp -- "$script_dir/manifest.json" "$work/candidate/manifest.json"
mv -n -- "$work/candidate" "$candidate"
test ! -d "$work/candidate" || { echo 'Candidate destination already exists.' >&2; exit 2; }
printf 'Staged unqualified XQuartz candidate for native review at %s\n' "$candidate"
