set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 570 470 click 1; sleep 2; import -window root /evidence/031-solo-advanced-current.png
