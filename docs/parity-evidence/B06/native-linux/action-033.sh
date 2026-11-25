set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 252 58 click 1; sleep .5; import -window root /evidence/068-idle-menu.png
