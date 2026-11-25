#!/usr/bin/env bash
set -euo pipefail
# The capabilities below apply only to fresh private user/network namespaces.
# No host sysctl, credentials, authorized_keys, interfaces or services are changed.
network=$(readlink /proc/self/ns/net)
exec bwrap --unshare-user --uid 0 --gid 0 --cap-add CAP_NET_BIND_SERVICE --cap-add CAP_NET_ADMIN \
 --unshare-net --unshare-pid --die-with-parent --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp \
 --setenv TERMCRAFT_V03_HOST_NETWORK "$network" --unsetenv DISPLAY --unsetenv WAYLAND_DISPLAY \
 --unsetenv WAYLAND_SOCKET --unsetenv XAUTHORITY --unsetenv SSH_AUTH_SOCK \
 -- /usr/bin/bash -c 'set -euo pipefail; [[ $(readlink /proc/self/ns/net) != "$TERMCRAFT_V03_HOST_NETWORK" ]]; ip link set lo up; exec "$@"' v03 "$@"
