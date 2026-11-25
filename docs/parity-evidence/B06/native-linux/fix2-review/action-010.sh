set -euo pipefail
export DISPLAY=:77
xdotool mousemove --sync 570 520 click 1; sleep 2; import -window root /evidence/020-map-seed3-history-followup.png; cat /proc/self/mountinfo > /evidence/mountinfo.txt; cat /proc/net/route > /evidence/network-routes.txt; ps -ef > /evidence/namespace-processes.txt; readlink /proc/self/ns/* > /evidence/namespace-identities.txt
