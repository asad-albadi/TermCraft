set -euo pipefail
export DISPLAY=:77
xdotool help windowclose; python -c "import importlib.util; print(importlib.util.find_spec(\"Xlib\"))"; xdotool key Escape; sleep .4; xdotool mousemove --sync 570 470 click 1; sleep .4; xdotool mousemove --sync 252 58 click 1; sleep .2; xdotool mousemove --sync 263 437 click 1; sleep .4; xdotool mousemove --sync 25 106 click 1; sleep .3; import -window root /evidence/055-solo-options-revisit.png
