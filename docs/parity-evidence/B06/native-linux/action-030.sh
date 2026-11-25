set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 332 70 click 1; sleep .3; xdotool mousemove --sync 400 106 click 1; sleep .5; import -window root /evidence/061-advanced-solo-solved.png; xdotool key Escape; sleep .4; xdotool mousemove --sync 570 470 click 1; sleep .4; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 513 click 1; sleep .5; xdotool mousemove --sync 635 206 click 1; sleep .2; xdotool mousemove --sync 340 206 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/062-map-75-unreasonable.png
