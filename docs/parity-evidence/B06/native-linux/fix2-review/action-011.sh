set -euo pipefail
export DISPLAY=:77
read ap win < /evidence/app2-window.txt; python3 /evidence/native-close.py "$win"; sleep 1; if kill -0 "$ap" 2>/dev/null; then echo owned-app-still-running; exit 1; fi; echo owned-app-exited-after-native-close; import -window root /evidence/021-final-close.png; touch /evidence/stop
