source /tmp/actions.sh
sha256sum "$owned"/* > /evidence/31-final-files.sha256
for ns in user pid net ipc mnt; do printf '%s ' "$ns"; readlink /proc/self/ns/$ns; done > /evidence/31-private-namespaces.txt
ps -eo pid,comm,args > /evidence/31-private-processes.txt
app
python /tmp/native-close.py "$(head -1 /evidence/appwin)"
sleep 1
if xdotool search --onlyvisible --name 'TermCraft — Remote workspace'; then exit 5; fi
shot 31-clean-native-close
same /tmp/legacy.termcraft-game "$owned/close.termcraft-game"
same /tmp/expected.termcraft-game "$owned/existing.termcraft-game"
no_temps
