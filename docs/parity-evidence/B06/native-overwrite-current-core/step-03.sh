source /tmp/actions.sh
click 511 470
sleep .8
shot 08-keep-existing-canceled
same /tmp/legacy.termcraft-game "$owned/existing.termcraft-game"
no_temps
sha256sum "$owned"/* > /evidence/08-files.sha256
