set -euo pipefail
export DISPLAY=:77
xdotool key Escape; sleep .25; xdotool mousemove --sync 570 470 click 1; sleep .3; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 209 click 1; sleep .4; import -window root /evidence/012-sudoku.png
