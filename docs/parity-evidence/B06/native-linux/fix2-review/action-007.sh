set -euo pipefail
export DISPLAY=:77
win=$(cut -d" " -f2 /evidence/app2-window.txt); xdotool windowsize "$win" 1350 980; sleep .3; import -window root /evidence/013-map-seed3-taller.png; xdotool key Escape; sleep .4; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 513 click 1; sleep .4; xdotool mousemove --sync 25 106 click 1; sleep .2; import -window root /evidence/014-map-reopened.png
