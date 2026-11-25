set -euo pipefail
export DISPLAY=:77
xdotool key Escape; sleep .4; xdotool mousemove --sync 570 470 click 1; sleep .4; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 551 click 1; sleep .5; import -window root /evidence/052-untangle.png
