launch fresh-final
click 565 100
xdotool mousemove 950 700 click --repeat 3 --delay 60 5
sleep 1
click 650 735
xdotool key ctrl+a
xdotool type --clearmodifiers --delay 25 '/usr/bin/bash'
click 410 815
shot fresh-final-saved
cp /evidence/fresh-final/workspace.json /evidence/fresh-final-saved-store.json
stop
launch fresh-final
shot fresh-final-restarted
xdotool key ctrl+shift+t
sleep 2
click 650 500
xdotool type --clearmodifiers --delay 20 "printf 'CLEAN_NATIVE_PTY_OK\\n' > /evidence/pty-marker.txt"
xdotool key Return
sleep 2
shot fresh-final-native-pty
cat /evidence/pty-marker.txt
cp /evidence/fresh-final/workspace.json /evidence/fresh-final-restarted-store.json
