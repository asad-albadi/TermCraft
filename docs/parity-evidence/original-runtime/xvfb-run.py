import subprocess,time,pathlib
with open('/work/reports/xvfb.log','w') as f:
 q=subprocess.Popen(['/work/xvfb/usr/bin/Xvfb',':0','-screen','0','1280x900x24','-nolisten','tcp','-ac'],stdout=f,stderr=f)
 try:
  for _ in range(100):
   if pathlib.Path('/tmp/.X11-unix/X0').exists():break
   if q.poll() is not None:raise RuntimeError('Xvfb failed')
   time.sleep(.05)
  wm=subprocess.Popen(['/work/twm/usr/bin/twm'],stdout=f,stderr=f)
  time.sleep(.5)
  raise SystemExit(subprocess.run(['/usr/bin/python','-I','/work/probe.py']).returncode)
 finally:
  if 'wm' in locals():wm.terminate();wm.wait(timeout=5)
  q.terminate();q.wait(timeout=5)
