#!/usr/bin/env bash
set -euo pipefail
# Invoked only after bwrap has isolated the network/user/PID namespaces.
[[ $(readlink /proc/self/ns/net) != "$TERMCRAFT_TEST_HOST_NETWORK" ]]
ip link set lo up
ip link add tcxdm0 type veth peer name tcxdm1
ip addr add 192.0.2.1/30 dev tcxdm0
ip link set tcxdm0 up
ip link set tcxdm1 up
exec "$@"
