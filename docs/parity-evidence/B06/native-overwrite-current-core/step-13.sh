source /tmp/actions.sh
app
xdotool mousemove --sync 120 100
sleep 2
shot 27-redraw
xdotool getwindowgeometry "$(head -1 /evidence/appwin)" > /evidence/27-geometry.txt
