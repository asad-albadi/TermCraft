set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 68 516 mousedown 1; sleep .15; xdotool mousemove --sync 110 540; sleep .15; xdotool mouseup 1; sleep .4; import -window root /evidence/053-untangle-drag.png; xdotool mousemove --sync 290 31 click 1; sleep 3; import -window root /evidence/054-save-chooser-probe.png; xdotool search --onlyvisible --name . getwindowname; cat /evidence/app.log
