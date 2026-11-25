set -euo pipefail
export DISPLAY=:77
win=$(cut -d" " -f2 /evidence/app2-window.txt); python /evidence/native-close.py "$win"; sleep .7; import -window root /evidence/088-native-close-unsaved-guard.png; xdotool mousemove --sync 735 470 click 1; sleep .4; import -window root /evidence/089-native-close-keep-playing.png; python /evidence/native-close.py "$win"; sleep .5; xdotool mousemove --sync 570 470 click 1; sleep .7; import -window root /evidence/090-native-close-discard-nextguard.png
