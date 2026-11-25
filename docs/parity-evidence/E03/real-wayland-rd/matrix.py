from gi.repository import Gio,GLib
import socket,threading,struct,fcntl
from PIL import Image
import os,sys,pathlib,subprocess,time,json,signal,shutil,ctypes
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
 for d in ['/tmp/real-config/xdg-desktop-portal','/tmp/real-data/xdg-desktop-portal/portals','/tmp/real-home','/tmp/empty','/tmp/schemas']:pathlib.Path(d).mkdir(parents=True,exist_ok=True)
 for schemas in [pathlib.Path('/usr/share/glib-2.0/schemas'),B/'tools/usr/share/glib-2.0/schemas']:
  for p in schemas.glob('*.xml'):shutil.copy2(p,'/tmp/schemas')
 cmd(['glib-compile-schemas','/tmp/schemas'])
 compositor=start(['mutter','--headless','--wayland','--no-x11','--virtual-monitor','800x600','--debug-control','--mutter-plugin='+str(B/'tools/usr/lib/mutter-51/plugins/libdefault.so')],'mutter')
 sockpath=wait(lambda:next(pathlib.Path('/tmp/real-runtime').glob('wayland-*'),None));os.environ['WAYLAND_DISPLAY']=sockpath.name
 for service,path in [('org.gnome.Mutter.RemoteDesktop','/org/gnome/Mutter/RemoteDesktop'),('org.gnome.Mutter.ScreenCast','/org/gnome/Mutter/ScreenCast'),('org.gnome.Mutter.DebugControl','/org/gnome/Mutter/DebugControl'),('org.gnome.Mutter.DisplayConfig','/org/gnome/Mutter/DisplayConfig')]:
  (R/(service+'.xml')).write_text(cmd(['gdbus','introspect','--session','--dest',service,'--object-path',path]))
 (R/'display-state.txt').write_text(cmd(['gdbus','call','--session','--dest','org.gnome.Mutter.DisplayConfig','--object-path','/org/gnome/Mutter/DisplayConfig','--method','org.gnome.Mutter.DisplayConfig.GetCurrentState']))
 start(['pipewire'],'pipewire');wait(lambda:pathlib.Path('/tmp/real-runtime/pipewire-0').exists());start(['wireplumber'],'wireplumber')
 shutil.copy2(B/'tools/usr/share/xdg-desktop-portal/portals/gnome.portal','/tmp/real-data/xdg-desktop-portal/portals/gnome.portal')
 os.environ['XDG_DESKTOP_PORTAL_DIR']='/tmp/real-data/xdg-desktop-portal/portals'
 pathlib.Path('/tmp/real-config/xdg-desktop-portal/portals.conf').write_text('[preferred]\ndefault=gnome\n')
 start([B/'tools/usr/lib/xdg-desktop-portal-gnome','-v'],'gnome-portal');time.sleep(.8);start(['/usr/lib/xdg-desktop-portal','-v'],'portal');time.sleep(2)
 (R/'portal-introspection.xml').write_text(cmd(['gdbus','introspect','--session','--dest','org.freedesktop.portal.Desktop','--object-path','/org/freedesktop/portal/desktop']))
 conn=Gio.bus_get_sync(Gio.BusType.SESSION,None)
 def db(dest,path,iface,method,sig=None,params=None):
  return conn.call_sync(dest,path,iface,method,GLib.Variant(sig,params) if sig else None,None,Gio.DBusCallFlags.NONE,3000,None).unpack()
 rdpath=db('org.gnome.Mutter.RemoteDesktop','/org/gnome/Mutter/RemoteDesktop','org.gnome.Mutter.RemoteDesktop','CreateSession')[0]
 (R/'rd-session.xml').write_text(db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.freedesktop.DBus.Introspectable','Introspect')[0])
 db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','Start')
 scpath=db('org.gnome.Mutter.ScreenCast','/org/gnome/Mutter/ScreenCast','org.gnome.Mutter.ScreenCast','CreateSession','(a{sv})',({},))[0]
 (R/'sc-session.xml').write_text(db('org.gnome.Mutter.ScreenCast',scpath,'org.freedesktop.DBus.Introspectable','Introspect')[0])
 stpath=db('org.gnome.Mutter.ScreenCast',scpath,'org.gnome.Mutter.ScreenCast.Session','RecordMonitor','(sa{sv})',('Meta-0',{}))[0]
 (R/'sc-stream.xml').write_text(db('org.gnome.Mutter.ScreenCast',stpath,'org.freedesktop.DBus.Introspectable','Introspect')[0])
 db('org.gnome.Mutter.ScreenCast',scpath,'org.gnome.Mutter.ScreenCast.Session','Start')
 nodes=json.loads(cmd(['pw-dump']))
 (R/'pw-before.json').write_text(json.dumps(nodes,indent=2))
 node=next(n for n in nodes if n.get('type')=='PipeWire:Interface:Node' and n.get('info',{}).get('props',{}).get('node.name')=='mutter')
 v=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);v.connect('/tmp/real-runtime/pipewire-0');a,z=socket.socketpair();vfd=fcntl.fcntl(v.fileno(),fcntl.F_DUPFD_CLOEXEC,20);zfd=fcntl.fcntl(z.fileno(),fcntl.F_DUPFD_CLOEXEC,20)
 helper=subprocess.Popen(['/bin/sh','-c','exec 3<&"$1" 4>&"$2"; exec "$3" "$4" "$5"','owned-fixture',str(vfd),str(zfd),str(B/'package/termcraft-wayland-engine'),str(node['id']),str(node['info']['props']['object.serial'])],pass_fds=(vfd,zfd),stdout=open(R/'fixture-helper.log','wb'),stderr=subprocess.STDOUT);owned.append(('fixture-helper',helper));os.close(vfd);os.close(zfd);z.close();v.close()
 latest=[]
 def exact(n):
  data=b''
  while len(data)<n:
   chunk=a.recv(n-len(data))
   if not chunk:raise EOFError()
   data+=chunk
  return data
 def reader():
  try:
   while True:
    h=exact(12);magic,w,hgt,size=struct.unpack('<4sHHI',h);assert magic==b'TVF1';data=exact(size);latest[:]=[w,hgt,data]
  except Exception as e:print('fixture screenshot reader ended',repr(e),flush=True)
 threading.Thread(target=reader,daemon=True).start()
 def screenshot(name):
  wait(lambda:len(latest)==3,8);w,h,data=latest.copy();Image.frombytes('RGBA',(w,h),data).save(R/(name+'.png'))
 def pixel(x,y):
  if not latest:return None
  w,h,data=latest.copy();return tuple(data[(y*w+x)*4:(y*w+x)*4+4])
 # Establish an owned fixture keyboard before mapping the receiver, modelling an already-present desktop keyboard.
 for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyKeyboardKeysym','(ub)',(65505,down));time.sleep(.1)
 (R/'fixture-keyboard-primer.json').write_text(json.dumps({'keysym':65505,'events':['down','up'],'time':time.monotonic(),'before_target_map':True,'production_provider_not_started':True}))
 target_env=os.environ.copy();target_env['WAYLAND_DEBUG']='client'
 target=start(['python','-I',B/'owned-target.py',R/'target'],'owned-target',target_env);wait(lambda:pixel(400,300)==(38,209,145,255),12);time.sleep(.5);screenshot('owned-target')
 wait(lambda:'wl_keyboard' in (R/'owned-target.log').read_text() and '.enter(' in (R/'owned-target.log').read_text(),5)
 (R/'keyboard-ready-before-provider.json').write_text(json.dumps({'time':time.monotonic(),'wire_lines':[l for l in (R/'owned-target.log').read_text().splitlines() if 'wl_keyboard' in l or ('wl_seat' in l and 'capabilities' in l)]}))
 def fixture_click(x,y):
  with (R/'fixture-consent-actions.jsonl').open('a') as f:f.write(json.dumps({'x':x,'y':y,'time':time.monotonic(),'purpose':'private native consent UI only'})+'\n')
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(-10000.,-10000.));time.sleep(.1)
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(float(x),float(y)));time.sleep(.1)
  for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerButton','(ib)',(272,down));time.sleep(.1)
 def action(out,n,value):
  started=time.monotonic();tmp=out/f'cmd-{n}.tmp';tmp.write_text(json.dumps(value));tmp.replace(out/f'cmd-{n}.json');wait(lambda:(out/f'ack-{n}.json').exists(),8)
  for attempt in range(20):
   try:ack=json.loads((out/f'ack-{n}.json').read_text());break
   except json.JSONDecodeError:time.sleep(.01)
  (out/f'timing-{n}.json').write_text(json.dumps({'start':started,'done':time.monotonic(),'action':value}));print(out.name,value,ack,flush=True)
  if value['kind']=='capture' and 'error' not in ack:Image.frombytes('RGBA',(ack['width'],ack['height']),(out/f'frame-{n}.rgba').read_bytes()).save(out/f'provider-capture-{n}.png')
  time.sleep(.18);return ack
 for mode in ['control','viewonly','control-denied','deny','cancel']:
  out=R/mode;out.mkdir();started=time.monotonic();provider=start([B/'package/real-wayland-rd-provider-private','viewonly' if mode=='viewonly' else 'control',out],mode)
  time.sleep(.5);wait(lambda:pixel(400,300)!=(38,209,145,255),8);time.sleep(.3);screenshot(mode+'-native-consent')
  if mode=='cancel':(out/'cancel').touch()
  elif mode=='deny':fixture_click(117,73)
  else:
   if mode=='control':fixture_click(650,154);time.sleep(.8);screenshot('control-interaction-enabled')
   fixture_click(685,73)
  if mode in ['control','viewonly']:
   wait(lambda:(out/'opened').exists() or (out/'result.json').exists(),8)
   if not (out/'opened').exists():raise RuntimeError(mode+' open failed: '+(out/'result.json').read_text())
   wait(lambda:pixel(400,300)==(38,209,145,255),8);time.sleep(.5);screenshot(mode+'-after-share')
   assert 'error' not in action(out,0,{'kind':'capture'})
   if mode=='control':
    actions=[{'kind':'pointer','buttons':0,'x':250,'y':250},{'kind':'pointer','buttons':1,'x':250,'y':250},{'kind':'pointer','buttons':0,'x':250,'y':250},{'kind':'key','down':True,'keysym':97},{'kind':'key','down':False,'keysym':97},{'kind':'key','down':True,'keysym':65505},{'kind':'pointer','buttons':1,'x':300,'y':280},{'kind':'release'},{'kind':'key','down':True,'keysym':98},{'kind':'pointer','buttons':1,'x':350,'y':300},{'kind':'stop'}]
    for n,v in enumerate(actions,1):assert 'error' not in action(out,n,v)
   else:
    before=(R/'target/events.jsonl').read_text()
    for n,v in enumerate([{'kind':'key','down':True,'keysym':122},{'kind':'pointer','buttons':1,'x':420,'y':310}],1):assert 'view-only' in action(out,n,v)['error']
    after=(R/'target/events.jsonl').read_text();(out/'input-observation.json').write_text(json.dumps({'before':before,'after':after,'unchanged':before==after}));assert before==after
    assert 'error' not in action(out,3,{'kind':'stop'})
  wait(lambda:provider.poll() is not None,8)
  assert provider.returncode==0
  (out/'lifecycle.json').write_text(json.dumps({'total_seconds':time.monotonic()-started,'returncode':provider.returncode}))
  if (out/'result.json').exists():print(mode,(out/'result.json').read_text(),flush=True)
  time.sleep(.5)
 assert compositor.poll() is None and target.poll() is None
 (R/'sibling-survival.json').write_text(json.dumps({'compositor':compositor.poll(),'owned_target':target.poll()}))
 (R/'processes-after-providers.txt').write_text(cmd(['ps','-eo','pid,ppid,comm,args']))
 db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','Stop')
 db('org.gnome.Mutter.ScreenCast',scpath,'org.gnome.Mutter.ScreenCast.Session','Stop')
try:main()
finally:
 for name,p in reversed(owned):
  if p.poll() is None:p.terminate()
 for name,p in reversed(owned):
  try:p.wait(timeout=3)
  except subprocess.TimeoutExpired:p.kill();p.wait()
 (R/'cleanup.json').write_text(json.dumps([{'name':name,'pid':p.pid,'returncode':p.returncode} for name,p in owned],indent=2))
