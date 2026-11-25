set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 252 58 click 1; sleep .4; import -window root /evidence/001-menu.png
