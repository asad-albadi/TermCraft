#!/bin/sh
echo invoked >> "/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/real-wayland-portal/reports/chooser.log"
case "$(cat "/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/real-wayland-portal/reports/choice")" in
accept) echo HEADLESS-1;;
deny) exit 1;;
wait) sleep 20;;
esac
