import re,os,sys,pathlib,subprocess,time,json,signal,hashlib,shutil,ctypes
from PIL import Image,ImageChops,ImageStat
ctypes.CDLL(None).prctl(36,1,0,0,0)
B=pathlib.Path(__file__).resolve().parent;APP=B.parents[2];R=B/'reports';R.mkdir(exist_ok=False);owned=[]
def start(args,name,env=None):
 p=subprocess.Popen([str(a) for a in args],stdout=open(R/(name+'.log'),'wb'),stderr=subprocess.STDOUT,env=env);owned.append((name,p));return p
def wait(fn,seconds=15):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  result=fn()
  if result:return result
  time.sleep(.05)
 raise RuntimeError('timeout '+repr(fn))
def cmd(args):return subprocess.check_output([str(x) for x in args],text=True)
def main():
 assert os.readlink('/proc/self/ns/net')!=os.environ['REAL_HOST_NET']
 (R/'isolation.json').write_text(json.dumps({'private_net':os.readlink('/proc/self/ns/net'),'host_net':os.environ['REAL_HOST_NET'],'display':os.environ.get('DISPLAY'),'wayland_before':os.environ.get('WAYLAND_DISPLAY'),'home':os.environ['HOME'],'dev':os.listdir('/dev'),'pid':os.getpid()}))
 for d in ['/tmp/real-config/xdg-desktop-portal','/tmp/real-data/xdg-desktop-portal/portals','/tmp/real-home','/tmp/empty']:pathlib.Path(d).mkdir(parents=True,exist_ok=True)
 wlr=APP/'target/wayland-real-portal-inputs/extracted'
 shutil.copy2(wlr/'usr/share/xdg-desktop-portal/portals/wlr.portal','/tmp/real-data/xdg-desktop-portal/portals/wlr.portal')
 pathlib.Path('/tmp/real-config/xdg-desktop-portal/portals.conf').write_text('[preferred]\ndefault=wlr\norg.freedesktop.impl.portal.ScreenCast=wlr\n')
 cfg=R/'sway.conf';cfg.write_text('output HEADLESS-1 mode 640x480\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\ndefault_border none\n')
 sway=start(['sway','--unsupported-gpu','-c',cfg],'sway');socket=wait(lambda:next(pathlib.Path('/tmp/real-runtime').glob('wayland-*'),None));os.environ['WAYLAND_DISPLAY']=socket.name;os.environ['SWAYSOCK']=str(wait(lambda:next(pathlib.Path('/tmp/real-runtime').glob('sway-ipc*sock'),None)))
 start(['pipewire'],'pipewire');wait(lambda:pathlib.Path('/tmp/real-runtime/pipewire-0').exists());start(['wireplumber'],'wireplumber')
 os.environ['XDG_DESKTOP_PORTAL_DIR']='/tmp/real-data/xdg-desktop-portal/portals'
 chooser=pathlib.Path('/tmp/chooser.sh');chooser.write_text('#!/bin/sh\necho invoked >> "'+str(R/'chooser.log')+'"\ncase "$(cat "'+str(R/'choice')+'")" in\naccept) echo HEADLESS-1;;\ndeny) exit 1;;\nwait) sleep 2;;\nesac\n');chooser.chmod(0o755)
 config=R/'wlr.conf';config.write_text('[screencast]\nchooser_type=simple\nchooser_cmd='+str(chooser)+'\nmax_fps=15\n')
 backend=start([wlr/'usr/lib/xdg-desktop-portal-wlr','-l','TRACE','-c',config],'wlr')
 start(['/usr/lib/xdg-desktop-portal','-v'],'portal');time.sleep(2)
 (R/'introspection.xml').write_text(cmd(['gdbus','introspect','--session','--dest','org.freedesktop.portal.Desktop','--object-path','/org/freedesktop/portal/desktop']))
 peer=start(['zenity','--info','--title=PRIVATE PORTAL TARGET 640x480','--text=TermCraft REAL Wayland Portal\nOwned compositor only\nScreenCast pixel qualification','--width=500','--height=300'],'owned-target');time.sleep(1)
 cmd(['grim',R/'owned-target.png'])
 for mode,choice in [('control','accept'),('deny','deny'),('cancel','wait'),('revoke','accept'),('normal-drop','accept'),('capture','accept')]:
  out=R/mode;out.mkdir(exist_ok=False);(R/'choice').write_text(choice)
  p=start([B/'package/real-wayland-portal-provider-private',mode,out],mode)
  if mode=='cancel':time.sleep(.3);(out/'cancel').touch()
  if mode in ['capture','revoke','normal-drop']:
   wait(lambda:(out/'captured').exists() or (out/'result.json').exists())
   if (out/'frame.json').exists():
    meta=json.loads((out/'frame.json').read_text());img=Image.frombytes('RGBA',(meta['width'],meta['height']),(out/'frame.rgba').read_bytes());img.save(out/'captured.png');ref=Image.open(R/'owned-target.png').convert('RGBA');diff=ImageStat.Stat(ImageChops.difference(img,ref));(out/'pixel-comparison.json').write_text(json.dumps({'mean_abs_diff_rgba':diff.mean,'dimensions':img.size,'reference':ref.size,'unique_colors':len(img.getcolors(640*480) or [])}))
    if mode=='capture':backend.terminate();backend.wait(timeout=3)
    elif mode=='revoke':
     session=re.findall(r'session_handle: (\S+)',(R/'wlr.log').read_text())[-1]
     (out/'revoke-call.txt').write_text(cmd(['gdbus','call','--session','--dest','org.freedesktop.impl.portal.desktop.wlr','--object-path',session,'--method','org.freedesktop.impl.portal.Session.Close']))
    time.sleep(.2);(out/'continue').touch()
  wait(lambda:p.poll() is not None,10)
  assert (out/'result.json').exists(),mode
  print(mode,(out/'result.json').read_text(),flush=True)
  if mode=='cancel':time.sleep(2)
 assert sway.poll() is None and peer.poll() is None
 (R/'remaining-processes.json').write_text(cmd(['ps','-eo','pid,ppid,comm,args']))
 (R/'sibling-survival.json').write_text(json.dumps({'sway':sway.poll(),'owned_target':peer.poll()}))
try:main()
finally:
 cleanup=[]
 for name,p in reversed(owned):
  if p.poll() is None:p.terminate()
 for name,p in reversed(owned):
  try:p.wait(timeout=3)
  except subprocess.TimeoutExpired:p.kill();p.wait()
  cleanup.append({'name':name,'pid':p.pid,'returncode':p.returncode})
 (R/'cleanup.json').write_text(json.dumps(cleanup,indent=2))
