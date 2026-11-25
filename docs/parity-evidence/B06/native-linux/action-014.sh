set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 100 410 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep .25; xdotool mousemove --sync 570 470 click 1; sleep 1; import -window root /evidence/029-solo-advanced.png; xdotool mousemove --sync 332 70 click 1; sleep .2; xdotool mousemove --sync 400 106 click 1; sleep .7; import -window root /evidence/030-solo-advanced-solved.png
