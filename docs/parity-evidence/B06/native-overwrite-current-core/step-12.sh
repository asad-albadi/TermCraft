source /tmp/actions.sh
sleep 2
shot 26-current-state
xdotool search --onlyvisible --name . getwindowname > /evidence/26-windows.txt
ps -eo pid,comm,args > /evidence/26-processes.txt
