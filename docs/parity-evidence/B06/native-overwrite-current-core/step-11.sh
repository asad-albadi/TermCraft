source /tmp/actions.sh
click 740 470
shot 23-load-keep-playing
app
click 361 31
click 576 470
wait_gtk 1
gtk_focus
xdotool key Escape
wait_gtk 0
sleep .8
shot 24-load-chooser-canceled
sha256sum "$owned"/* > /evidence/24-files.sha256
same /evidence/21-before-cancel-files.sha256 /evidence/24-files.sha256
app
click 361 31
sleep .5
shot 25-load-again-guard
