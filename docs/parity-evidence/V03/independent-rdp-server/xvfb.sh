#!/bin/bash
set -eu
B="/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/independent-rdp-server"
"/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/validation/v08-clean-namespace/rootfs/usr/bin/Xvfb" :0 -screen 0 1280x900x24 -nolisten tcp > "$B/client-xvfb.log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || :; wait "$pid" || :' EXIT
export DISPLAY=:0
for i in {1..100}; do [[ ! -S /tmp/.X11-unix/X0 ]] || break; sleep .03; done
timeout 300 "$@"
