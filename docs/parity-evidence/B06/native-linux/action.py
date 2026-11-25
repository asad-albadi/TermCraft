"""Append one exact owned-display command. Run only while session.sh owns Xvfb."""
import sys,time
from pathlib import Path
out=Path(__file__).resolve().parent
n=1
while (out/f'action-{n:03}.sh').exists(): n+=1
p=out/f'action-{n:03}.sh'
p.write_text('set -euo pipefail\nexport DISPLAY=:77\n'+sys.argv[1]+'\n')
end=time.monotonic()+55
while time.monotonic()<end:
 if p.with_suffix('.done').exists():
  print(p.name); print(p.with_suffix('.log').read_text());
  if p.with_suffix('.exit').exists(): raise SystemExit(p.with_suffix('.exit').read_text())
  break
 time.sleep(.2)
else: raise SystemExit('Action timed out; do not enqueue dependent commands')
