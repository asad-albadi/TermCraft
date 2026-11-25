launch upgrade
click 565 100
xdotool mousemove 950 700 click --repeat 3 --delay 60 5
sleep 1
shot legacy-before-save
click 410 815
shot legacy-saved
cp /evidence/upgrade/workspace.json /evidence/upgrade-old-saved-store.json
