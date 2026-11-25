set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 830 549 click 1; sleep 1; xdotool mousemove --sync 600 500 click 1; xdotool type --clearmodifiers --delay 20 "python /evidence/pty-capture.py"; xdotool key Return; sleep .5; xdotool key b; xdotool mousemove --sync 252 58 click 1; sleep .4; xdotool mousemove --sync 242 641 click 1; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .3; import -window root /evidence/082-fresh-menu-checkbox.png
