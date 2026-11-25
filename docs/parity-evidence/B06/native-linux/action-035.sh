set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 252 58 click 1; sleep .4; xdotool mousemove --sync 250 679 mousedown 1; sleep .2; xdotool mousemove --sync 239 679; sleep .2; xdotool mouseup 1; sleep .4; import -window root /evidence/073-idle-drag-minimum.png
