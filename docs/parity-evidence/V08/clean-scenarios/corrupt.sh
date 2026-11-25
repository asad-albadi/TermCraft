launch corrupt
shot corrupt-startup
click 565 100
xdotool mousemove 950 700 click --repeat 3 --delay 60 5
sleep 1
click 410 815
shot corrupt-save-blocked
cp /evidence/corrupt/workspace.json /evidence/corrupt-after-save.bin
stop
launch corrupt
shot corrupt-restarted
python -c "from pathlib import Path; assert Path('/evidence/corrupt-input.bin').read_bytes() == Path('/evidence/corrupt/workspace.json').read_bytes(); print('CORRUPT_BYTES_PRESERVED')"
