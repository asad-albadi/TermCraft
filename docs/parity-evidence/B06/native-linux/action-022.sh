set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 58 69; sleep .2; xdotool mousedown 1; sleep .15; xdotool mouseup 1; sleep .7; import -window root /evidence/049-map-confirm-slow.png
