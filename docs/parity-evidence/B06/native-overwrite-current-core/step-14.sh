source /tmp/actions.sh
printf 'invalid owned saved game fixture\n' > "$owned/invalid.termcraft-game"
sha256sum "$owned"/* > /evidence/28-before-invalid-files.sha256
click 575 470
wait_gtk 1
gtk_focus
xdotool key ctrl+l
sleep .3
xdotool type --clearmodifiers "$owned/invalid.termcraft-game"
sleep .6
xdotool key Return
wait_gtk 0
sleep .8
xdotool mousemove --sync 100 100
sleep .3
shot 28-invalid-load-preserved
sha256sum "$owned"/* > /evidence/28-after-invalid-files.sha256
same /evidence/28-before-invalid-files.sha256 /evidence/28-after-invalid-files.sha256
no_temps
