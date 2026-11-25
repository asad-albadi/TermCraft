source /tmp/actions.sh
click 640 470
sleep .8
shot 11-race-target-rejected
same /evidence/race-second-writer.txt "$owned/race.termcraft-game"
same /tmp/legacy.termcraft-game "$owned/race.old"
no_temps
sha256sum "$owned"/* > /evidence/11-files.sha256
