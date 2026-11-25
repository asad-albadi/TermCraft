set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 80 143 click 1; sleep 2; import -window root /evidence/091-native-final-close.png; apppid=$(cut -d" " -f1 /evidence/app2-window.txt); if kill -0 "$apppid" 2>/dev/null; then echo app-still-running; exit 1; else echo owned-app-exited-after-consent; fi; ps -eo pid,ppid,args > /evidence/namespace2-final-processes.txt; cp /tmp/store/workspace.json /evidence/workspace2-after.json; touch /evidence/stop
