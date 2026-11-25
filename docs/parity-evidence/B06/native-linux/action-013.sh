set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 500 106 click 1; sleep .25; xdotool mousemove --sync 110 145 click 1; sleep .3; import -window root /evidence/028-solo-preset-menu.png
