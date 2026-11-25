set -euo pipefail
export DISPLAY=:77
xdotool key Escape; sleep .25; xdotool mousemove --sync 570 470 click 1; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 171 click 1; sleep .4; import -window root /evidence/010-ctris.png; xdotool mousemove --sync 200 230 click 1; xdotool key Left Up space; sleep .4; import -window root /evidence/011-ctris-move.png
