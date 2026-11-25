source /tmp/actions.sh
click 739 470
sleep .5
shot 29-native-close-keep-playing
same /tmp/legacy.termcraft-game "$owned/close.termcraft-game"
same /tmp/expected.termcraft-game "$owned/existing.termcraft-game"
no_temps
sha256sum "$owned"/* > /evidence/29-files.sha256
app
click 290 31
wait_gtk 1
gtk_focus
click 150 27
xdotool key ctrl+a
xdotool type --clearmodifiers "$owned/final-state.termcraft-game"
sleep .5
xdotool key Return
wait_gtk 0
sleep .8
xdotool mousemove --sync 100 100
shot 30-final-state-saved
cp "$owned/final-state.termcraft-game" /evidence/final-state.termcraft-game
no_temps
