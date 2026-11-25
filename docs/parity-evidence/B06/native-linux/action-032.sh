set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 600 500 click 1; xdotool type --clearmodifiers --delay 25 "python /evidence/pty-capture.py"; xdotool key Return; sleep 1; xdotool key b; sleep 32; import -window root /evidence/066-idle-default-off-32s.png; xdotool mousemove --sync 252 58 click 1; sleep .3; xdotool mousemove --sync 242 641 click 1; sleep .3; xdotool mousemove --sync 246 679 click 1; sleep .3; import -window root /evidence/067-idle-config30.png
