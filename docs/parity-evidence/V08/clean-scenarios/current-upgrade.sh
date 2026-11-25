launch upgrade
shot upgrade-current-before-save
cp /evidence/upgrade/workspace.json /evidence/upgrade-current-before-save-store.json
click 565 100
xdotool mousemove 950 700 click --repeat 3 --delay 60 5
sleep 1
click 410 815
shot upgrade-current-saved
cp /evidence/upgrade/workspace.json /evidence/upgrade-current-saved-store.json
stop
launch upgrade
shot upgrade-current-restarted
cp /evidence/upgrade/workspace.json /evidence/upgrade-current-restarted-store.json
