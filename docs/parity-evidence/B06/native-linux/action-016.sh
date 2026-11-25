set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 340 206 click 1; sleep .2; xdotool mousemove --sync 1000 206 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep .5; xdotool mousemove --sync 570 470 click 1; sleep 2; import -window root /evidence/032-solo-jigsaw.png
