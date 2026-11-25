source /tmp/actions.sh
overwrite_path close
gtk_replace
shot 19-app-close-overwrite-modal
same /tmp/legacy.termcraft-game "$owned/close.termcraft-game"
app
python /tmp/native-close.py "$(head -1 /evidence/appwin)"
sleep .8
shot 20-native-close-changes-guard
same /tmp/legacy.termcraft-game "$owned/close.termcraft-game"
ps -eo pid,comm,args > /evidence/private-processes.txt
for ns in user pid net ipc mnt; do printf '%s ' "$ns"; readlink /proc/self/ns/$ns; done > /evidence/private-namespaces.txt
