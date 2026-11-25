set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 570 470; sleep .2; xdotool mousedown 1; sleep .15; xdotool mouseup 1; sleep 2; import -window root /evidence/050-map-unreasonable-result.png
