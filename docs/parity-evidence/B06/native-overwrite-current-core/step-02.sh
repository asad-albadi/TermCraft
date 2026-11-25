source /tmp/actions.sh
overwrite_path existing
shot 06-gtk-replace-confirmation
gtk_replace
shot 07-app-overwrite-modal
xdotool search --onlyvisible --name . getwindowname > /evidence/07-visible-x11-windows.txt
same /tmp/legacy.termcraft-game "$owned/existing.termcraft-game"
