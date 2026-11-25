source /tmp/actions.sh
click 651 479
sleep .8
shot 14-approved-overwrite-saved
same /tmp/expected.termcraft-game "$owned/existing.termcraft-game"
cp "$owned/existing.termcraft-game" /evidence/approved.termcraft-game
no_temps
sha256sum "$owned"/* > /evidence/14-files.sha256
