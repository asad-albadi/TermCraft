#!/bin/bash
set -euo pipefail
base=$(cd "$(dirname "$0")" && pwd)
app=$(cd "$base/../../.." && pwd)
export REAL_HOST_NET=$(readlink /proc/self/ns/net)
exec bwrap --unshare-net --unshare-pid --die-with-parent --ro-bind / / --dev /dev --proc /proc --tmpfs /home --ro-bind "$app" "$app" --bind "$base" "$base" --tmpfs /run --perms 01777 --tmpfs /tmp --perms 0700 --dir /tmp/real-runtime --setenv XDG_RUNTIME_DIR /tmp/real-runtime --setenv XDG_CONFIG_HOME /tmp/real-config --setenv XDG_CONFIG_DIRS /tmp/empty --setenv XDG_DATA_HOME /tmp/real-data --setenv HOME /tmp/real-home --unsetenv DISPLAY --unsetenv WAYLAND_DISPLAY --unsetenv WAYLAND_SOCKET --unsetenv XAUTHORITY --unsetenv SWAYSOCK --unsetenv DBUS_SESSION_BUS_ADDRESS --unsetenv PIPEWIRE_REMOTE --setenv WLR_BACKENDS headless --setenv WLR_HEADLESS_OUTPUTS 1 --setenv WLR_RENDERER pixman --setenv LIBGL_ALWAYS_SOFTWARE 1 --setenv XDG_CURRENT_DESKTOP sway --setenv PATH "$app/target/agent-work/v06/tools/usr/bin:/usr/bin" --setenv LD_LIBRARY_PATH "$app/target/agent-work/v06/tools/usr/lib" -- dbus-run-session -- python -I "$base/matrix.py"
