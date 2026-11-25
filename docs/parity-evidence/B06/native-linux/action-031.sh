set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 147 478 click 1; sleep .15; xdotool key 2; sleep .3; import -window root /evidence/063-map-unreasonable-move.png; xdotool mousemove --sync 332 70 click 1; sleep .3; xdotool mousemove --sync 400 106 click 1; sleep .5; import -window root /evidence/064-map-unreasonable-solved.png; xdotool key Escape; sleep .4; xdotool mousemove --sync 570 470 click 1; sleep .4; xdotool mousemove --sync 830 549 click 1; sleep 2; import -window root /evidence/065-local-pty.png
