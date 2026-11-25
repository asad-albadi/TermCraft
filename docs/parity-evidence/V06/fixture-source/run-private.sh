#!/bin/bash
set -euo pipefail
base=$(cd "$(dirname "$0")" && pwd)
v06_host_net=$(readlink /proc/self/ns/net)
v06_host_sockets=$(stat -Lc '%d:%i' /tmp/.X11-unix)
exec bwrap --unshare-net --unshare-pid --die-with-parent --ro-bind / / --dev /dev --proc /proc --bind "$base" "$base" --perms 01777 --tmpfs /tmp --perms 01777 --dir /tmp/.X11-unix --perms 0700 --dir /tmp/v06-runtime --setenv V06_HOST_NET "$v06_host_net" --setenv V06_HOST_SOCKETS "$v06_host_sockets" --setenv XDG_RUNTIME_DIR /tmp/v06-runtime --setenv XDG_CONFIG_HOME /tmp/v06-config --setenv HOME /tmp/v06-home --unsetenv DISPLAY --unsetenv WAYLAND_DISPLAY --unsetenv WAYLAND_SOCKET --unsetenv XAUTHORITY --unsetenv SWAYSOCK --unsetenv DBUS_SESSION_BUS_ADDRESS --setenv WLR_BACKENDS headless --setenv WLR_HEADLESS_OUTPUTS 1 --setenv WLR_RENDERER pixman --setenv LIBGL_ALWAYS_SOFTWARE 1 --setenv PATH "$base/tools/usr/bin:/usr/bin" --setenv LD_LIBRARY_PATH "$base/tools/usr/lib" -- dbus-run-session -- python -I "$base/matrix.py" "$@"
