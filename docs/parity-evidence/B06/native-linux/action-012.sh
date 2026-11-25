set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 35 295 click 1; xdotool key 1; sleep .3; import -window root /evidence/025-solo-move.png; xdotool mousemove --sync 332 70 click 1; sleep .3; import -window root /evidence/026-solve-confirm.png; xdotool mousemove --sync 730 106 click 1; sleep .2; xdotool mousemove --sync 70 106 click 1; sleep .3; xdotool mousemove --sync 110 146 click 1; sleep .3; import -window root /evidence/027-solo-presets.png
