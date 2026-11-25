set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 25 106 click 1; sleep .3; xdotool mousemove --sync 585 206 click 1; sleep .2; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/056-solo-advanced-fail-first.png; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/057-solo-advanced-fail-second.png; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/058-solo-advanced-fail-third.png
