set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 340 206 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep .5; xdotool mousemove --sync 585 206 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/059-solo-advanced-seed3.png; xdotool mousemove --sync 315 31 click 1; sleep 3; import -window root /evidence/060-load-probe.png
