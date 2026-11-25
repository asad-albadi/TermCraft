source /tmp/actions.sh
overwrite_path race
shot 09-gtk-race-target-confirm
gtk_replace
shot 10-app-race-overwrite-modal
same /tmp/legacy.termcraft-game "$owned/race.termcraft-game"
mv -- "$owned/race.termcraft-game" "$owned/race.old"
printf 'Owned second writer changed the target while consent was pending.\n' > "$owned/race.termcraft-game"
cp -- "$owned/race.termcraft-game" /evidence/race-second-writer.txt
sha256sum "$owned/race.termcraft-game" "$owned/race.old" > /evidence/race-before-answer.sha256
