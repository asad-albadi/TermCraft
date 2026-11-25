set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 270 94 click 1; sleep .5; import -window root /evidence/002-soliterm.png
