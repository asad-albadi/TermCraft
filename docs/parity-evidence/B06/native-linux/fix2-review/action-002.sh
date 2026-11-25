set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 585 206 click 1; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/002-solo-seed2-failed.png; xdotool mousemove --sync 58 70 click 1; sleep 2; import -window root /evidence/003-solo-seed3-advanced.png
