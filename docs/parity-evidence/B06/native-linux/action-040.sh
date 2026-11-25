set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 250 679 mousedown 1; sleep .2; xdotool mousemove --sync 239 679; sleep .2; xdotool mouseup 1; sleep .3; import -window root /evidence/083-fresh-idle30-config.png; xdotool mousemove --sync 600 550 click 1; sleep 34; import -window root /evidence/084-fresh-idle-after34.png; xdotool getwindowfocus getwindowname; xdotool key x; sleep .7; xdotool type --clearmodifiers "y!"; sleep .3; import -window root /evidence/085-fresh-pty-after-wake.png; od -An -tx1 /evidence/pty-input.bin
