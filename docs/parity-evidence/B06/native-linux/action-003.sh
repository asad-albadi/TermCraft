set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 55 405 click 1; xdotool mousemove --sync 70 230 click 1; sleep .3; import -window root /evidence/003-soliterm-move.png; xdotool mousemove --sync 200 70 click 1; sleep .2; import -window root /evidence/004-soliterm-undo.png; xdotool mousemove --sync 264 70 click 1; sleep .2; import -window root /evidence/005-soliterm-redo.png; xdotool key Escape; sleep .3; import -window root /evidence/006-dirty-close.png; xdotool search --onlyvisible --name . getwindowname
