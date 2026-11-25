set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 795 800 click 5 click 5 click 5 click 5 click 5 click 5; sleep .5; import -window root /evidence/051-map-error-bottom.png
