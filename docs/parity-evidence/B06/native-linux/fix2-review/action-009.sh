set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 200 70 click 1; sleep .3; import -window root /evidence/017-map-history-undo-after-failure.png; xdotool mousemove --sync 262 70 click 1; sleep .3; import -window root /evidence/018-map-history-redo-after-failure.png; xdotool mousemove --sync 58 70 click 1; sleep .3; import -window root /evidence/019-map-retry-tall-consent.png
