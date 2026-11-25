#!/usr/bin/env bash
set -euo pipefail
if (( $# < 2 )); then
  echo 'Usage: run_xserver_isolated.sh PACKAGE_ROOT TEST_EXECUTABLE [TEST_ARGUMENTS...]' >&2
  exit 2
fi
package=$(realpath -- "$1")
binary=$(realpath -- "$2")
shift 2
fixture=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/xserver_isolated.py
host_socket_dir=$(stat -Lc '%d:%i' /tmp/.X11-unix)
host_network=$(readlink /proc/self/ns/net)
exec bwrap --unshare-net --unshare-pid --die-with-parent \
  --ro-bind / / --dev /dev --proc /proc \
  --ro-bind "$package/x11/linux-x86_64/Xwayland" /usr/bin/Xwayland \
  --perms 01777 --tmpfs /tmp \
  --perms 01777 --dir /tmp/.X11-unix --perms 0700 --dir /tmp/wayland \
  --setenv TERMCRAFT_TEST_XSERVER_PACKAGE "$package" \
  --setenv TERMCRAFT_TEST_HOST_SOCKET_DIR "$host_socket_dir" \
  --setenv TERMCRAFT_TEST_HOST_NETWORK "$host_network" \
  --unsetenv WAYLAND_DISPLAY --unsetenv WAYLAND_SOCKET --unsetenv DISPLAY --unsetenv XAUTHORITY \
  --setenv WLR_BACKENDS headless --setenv WLR_HEADLESS_OUTPUTS 1 --setenv WLR_RENDERER pixman \
  --setenv XDG_RUNTIME_DIR /tmp/wayland \
  -- /usr/bin/cage -- /usr/bin/python -I "$fixture" "$binary" "$@"
