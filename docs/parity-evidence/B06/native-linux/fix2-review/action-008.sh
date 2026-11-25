set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 25 106 click 1; sleep .2; xdotool mousemove --sync 635 206 click 1; xdotool mousemove --sync 25 106 click 1; sleep .2; xdotool mousemove --sync 35 358 click 1; xdotool key 2; sleep .2; import -window root /evidence/015-map-history-before.png; xdotool mousemove --sync 58 70 click 1; sleep .2; xdotool mousemove --sync 570 520 click 1; sleep .3; import -window root /evidence/016-map-second-consent-position.png
