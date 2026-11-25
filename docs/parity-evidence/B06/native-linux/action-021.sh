set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 58 70 click 1; sleep .6; import -window root /evidence/047-map-new-confirm.png; xdotool mousemove --sync 570 470 click 1; sleep 1; import -window root /evidence/048-map-unreasonable-current.png
