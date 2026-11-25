set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 735 470 click 1; sleep .3; xdotool mousemove --sync 548 31 click 1; sleep .3; import -window root /evidence/007-library.png
