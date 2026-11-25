set -euo pipefail
mkdir -p /tmp/home /build/out
rustc -Vv
cargo -V
cat /etc/os-release
grep Cpus_allowed_list /proc/self/status
TERMCRAFT_OFFLINE=1 CARGO_BUILD_JOBS=2 python3 -u /src/scripts/package-release.py --platform linux --build-root /build --output /build/out
python3 /src/scripts/package-debian.py /build/out/TermCraft-0.1.0-linux-x86_64.tar.gz --output /build/out --debian-revision 3
# Auxiliary persistence fixture uses the exact release library, never substitutes a packaged executable.
test -f /build/package-app/release/libharbor_workspace.rlib
rustc --edition 2024 /driver/fixture.rs --extern harbor_workspace=/build/package-app/release/libharbor_workspace.rlib -L dependency=/build/package-app/release/deps -o /build/persistence-fixture
sha256sum /build/out/* /build/persistence-fixture
