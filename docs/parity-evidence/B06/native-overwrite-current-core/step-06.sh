source /tmp/actions.sh
overwrite_path existing
shot 12-gtk-approval-confirm
gtk_replace
shot 13-app-approval-overwrite-modal
same /tmp/legacy.termcraft-game "$owned/existing.termcraft-game"
