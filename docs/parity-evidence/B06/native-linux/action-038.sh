set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 242 641 click 1; sleep .4; import -window root /evidence/079-idle-on-thirty.png; xdotool key Escape; sleep 35; import -window root /evidence/080-idle-thirty-retry.png
