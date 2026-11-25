set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 252 58 click 1; sleep .3; xdotool mousemove --sync 263 437 click 1; sleep .5; xdotool mousemove --sync 25 106 click 1; sleep .3; import -window root /evidence/001-solo-options.png
