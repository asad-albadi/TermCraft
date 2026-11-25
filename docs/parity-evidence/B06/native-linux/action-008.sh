set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 111 195 click 1; sleep .15; xdotool key 1; sleep .25; import -window root /evidence/013-sudoku-move.png; xdotool key Escape; sleep .25; xdotool mousemove --sync 570 470 click 1; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 247 click 1; sleep .4; import -window root /evidence/014-nimesweeper.png; xdotool mousemove --sync 35 196 click 1; sleep .2; xdotool key f; sleep .25; import -window root /evidence/015-nimesweeper-move.png
