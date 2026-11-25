set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 35 482 click 1; xdotool key 2; sleep .3; xdotool mousemove --sync 635 206 click 1; sleep .2; import -window root /evidence/006-map-dirty-seed1.png; xdotool mousemove --sync 58 70 click 1; sleep .3; import -window root /evidence/007-map-new-consent.png
