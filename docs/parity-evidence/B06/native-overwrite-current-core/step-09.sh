source /tmp/actions.sh
sha256sum "$owned"/* > /evidence/21-before-cancel-files.sha256
app
click 290 31
wait_gtk 1
gtk_focus
xdotool key Escape
wait_gtk 0
sleep .8
shot 21-save-chooser-canceled
sha256sum "$owned"/* > /evidence/21-after-cancel-files.sha256
cmp /evidence/21-before-cancel-files.sha256 /evidence/21-after-cancel-files.sha256
no_temps
app
click 361 31
sleep .5
shot 22-load-unsaved-guard
