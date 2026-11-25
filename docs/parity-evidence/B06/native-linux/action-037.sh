set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 252 58 click 1; sleep .3; import -window root /evidence/077-idle-recheck.png; xdotool mousemove --sync 242 641 click 1; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .3; import -window root /evidence/078-idle-toggle-state.png
