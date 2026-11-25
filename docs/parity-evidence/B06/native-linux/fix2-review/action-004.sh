set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 25 106 click 1; sleep .3; import -window root /evidence/005-map-options-open.png
