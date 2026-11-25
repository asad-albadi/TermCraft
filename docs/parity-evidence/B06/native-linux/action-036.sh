set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 600 550 click 1; xdotool type --clearmodifiers --delay 20 "python /evidence/pty-capture.py"; xdotool key Return; sleep 1; xdotool key b; sleep 32; import -window root /evidence/074-idle-active30.png; xdotool key x; sleep .7; import -window root /evidence/075-idle-woken30.png; xdotool type --clearmodifiers "y!"; sleep .5; import -window root /evidence/076-pty-resumed30.png; od -An -tx1 /evidence/pty-input.bin
