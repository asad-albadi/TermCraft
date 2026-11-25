set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 415 480 click 1; xdotool key 2; sleep .3; import -window root /evidence/044-map-move.png; xdotool mousemove --sync 630 206 click 1; sleep .25; xdotool mousemove --sync 58 70 click 1; sleep .4; xdotool mousemove --sync 570 470 click 1; sleep 1; import -window root /evidence/045-map-unreasonable.png; xdotool mousemove --sync 332 70 click 1; sleep .3; xdotool mousemove --sync 400 106 click 1; sleep .6; import -window root /evidence/046-map-unreasonable-solved.png
