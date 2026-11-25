set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 362 679 keydown Control_L click 1 keyup Control_L; sleep .2; xdotool key ctrl+a; xdotool type --clearmodifiers 30; xdotool key Return; sleep .4; import -window root /evidence/069-idle-thirty-config.png; xdotool mousemove --sync 600 500 click 1; sleep 32; import -window root /evidence/070-idle-active.png; xdotool key x; sleep .7; import -window root /evidence/071-idle-woken.png; xdotool type --clearmodifiers "y!"; sleep .5; import -window root /evidence/072-pty-resumed.png; od -An -tx1 /evidence/pty-input.bin
