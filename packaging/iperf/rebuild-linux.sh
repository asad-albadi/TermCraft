#!/usr/bin/env bash
# Offline rebuild into a separate output directory; never changes approved pins.
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
if [[ $# != 1 ]]; then
    echo 'Usage: rebuild-linux.sh NEW_OUTPUT_DIRECTORY (Debian 12 x86_64 builder)' >&2
    exit 2
fi
if [[ $(uname -sm) != 'Linux x86_64' ]]; then
    echo 'Use a native Linux x86_64 builder.' >&2
    exit 1
fi
output=$(realpath -m -- "$1")
mkdir -- "$output"
python3 - "$repo_root" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
info = json.loads((root/'packaging/iperf/runtime-manifest.json').read_text())
for artifact in info['artifacts']:
    for suffix, field in [('-source.tar.gz', 'source_sha256'), ('-LICENSE', 'license_sha256')]:
        path = root/'runtime/linux-x86_64'/(artifact['tool'] + suffix)
        if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest() != artifact[field]:
            raise SystemExit(f'Pinned source/license mismatch: {path}')
PY
build_dir=$(mktemp -d)
trap 'rm -rf -- "$build_dir"' EXIT
for tool in iperf2 iperf3; do
    mkdir -- "$build_dir/$tool"
    # Only the archive whose digest was checked above reaches tar/configure.
    tar -xf "$repo_root/runtime/linux-x86_64/$tool-source.tar.gz" -C "$build_dir/$tool" --strip-components=1
    (
        cd -- "$build_dir/$tool"
        if [[ $tool == iperf2 ]]; then
            ./configure > "$output/$tool-configure.log" 2>&1
        else
            ./configure --disable-shared --enable-static --without-openssl > "$output/$tool-configure.log" 2>&1
        fi
        make -j2 > "$output/$tool-make.log" 2>&1
        if [[ $tool == iperf2 ]]; then
            cp -- src/iperf "$output/$tool"
        else
            cp -- src/iperf3 "$output/$tool"
        fi
    )
    "$output/$tool" --version > "$output/$tool-version.txt" 2>&1
    readelf --version-info "$output/$tool" > "$output/$tool-symbols.txt"
    ldd "$output/$tool" > "$output/$tool-libraries.txt"
done
{ cat /etc/os-release; cc --version; ldd --version; sha256sum "$output/iperf2" "$output/iperf3"; } > "$output/toolchain.txt"
echo "Candidates written to $output; qualify independently before changing approved binary pins."
