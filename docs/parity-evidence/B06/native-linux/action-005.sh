set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 50 131 click 1; sleep .25; xdotool mousemove --sync 570 470 click 1; sleep .3; import -window root /evidence/008-termwalk.png; xdotool mousemove --sync 40 200 click 1; sleep .2; xdotool key d; sleep .3; import -window root /evidence/009-termwalk-move.png
