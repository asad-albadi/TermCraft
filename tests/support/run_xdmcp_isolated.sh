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
keyboard_fixture=()
if [[ ${TERMCRAFT_TEST_MASK_GERMAN_SYMBOLS:-0} == 1 ]]; then
  keyboard_fixture=(--ro-bind /dev/null "$(realpath /usr/share/X11/xkb/symbols/de)")
fi
net_caps=(--cap-add CAP_NET_ADMIN)
fixture_env=()
if [[ ${TERMCRAFT_TEST_XDMCP_CHOOSER:-0} == 1 ]]; then
  # Real xdm listens on the standard privileged port in its private network only.
  net_caps+=(--cap-add CAP_NET_BIND_SERVICE)
  fixture_env=(--perms 0700 --dir /tmp/chooser-home --setenv HOME /tmp/chooser-home --unsetenv XENVIRONMENT)
fi
exec bwrap --unshare-user --uid 0 --gid 0 "${net_caps[@]}" --unshare-net --unshare-pid --die-with-parent \
  --ro-bind / / "${keyboard_fixture[@]}" --dev /dev --proc /proc \
  --ro-bind "$package/x11/linux-x86_64/Xwayland" /usr/bin/Xwayland \
  --perms 01777 --tmpfs /tmp "${fixture_env[@]}" \
  --perms 01777 --dir /tmp/.X11-unix --perms 0700 --dir /tmp/wayland \
  --setenv TERMCRAFT_TEST_XSERVER_PACKAGE "$package" \
  --setenv TERMCRAFT_TEST_HOST_SOCKET_DIR "$host_socket_dir" \
  --setenv TERMCRAFT_TEST_HOST_NETWORK "$host_network" \
  --unsetenv WAYLAND_DISPLAY --unsetenv WAYLAND_SOCKET --unsetenv DISPLAY --unsetenv XAUTHORITY \
  --setenv WLR_BACKENDS headless --setenv WLR_HEADLESS_OUTPUTS 1 --setenv WLR_RENDERER pixman \
  --setenv XDG_RUNTIME_DIR /tmp/wayland \
  -- /usr/bin/bash "$(dirname -- "$fixture")/xdmcp_private_net.sh" /usr/bin/cage -- /usr/bin/python -I "$fixture" "$binary" "$@"
