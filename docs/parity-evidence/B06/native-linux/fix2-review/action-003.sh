set -euo pipefail
export DISPLAY=:77
xdotool key Escape; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 513 click 1; sleep .4; xdotool mousemove --sync 25 106 click 1; sleep .3; import -window root /evidence/004-map-options.png
