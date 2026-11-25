from gi.repository import Gio,GLib
import socket,threading,struct,fcntl
from PIL import Image
import os,sys,pathlib,subprocess,time,json,signal,shutil,ctypes
ctypes.CDLL(None).prctl(36,1,0,0,0)
B=pathlib.Path(__file__).resolve().parent;APP=B.parents[2];R=B/'direct-native-comparison';R.mkdir(exist_ok=False);owned=[]
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
 target_env=os.environ.copy();target_env['WAYLAND_DEBUG']='client'
 target=start(['python','-I',B/'owned-target.py',R/'target'],'owned-target',target_env);wait(lambda:pixel(400,300)==(38,209,145,255),12);time.sleep(.5);screenshot('owned-target')
 def fixture_click(x,y):
  with (R/'fixture-consent-actions.jsonl').open('a') as f:f.write(json.dumps({'x':x,'y':y,'time':time.monotonic(),'purpose':'private native consent UI only'})+'\n')
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(-10000.,-10000.));time.sleep(.1)
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(float(x),float(y)));time.sleep(.1)
  for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerButton','(ib)',(272,down));time.sleep(.1)
 def action(out,n,value):
  started=time.monotonic();(out/f'cmd-{n}.json').write_text(json.dumps(value));wait(lambda:(out/f'ack-{n}.json').exists(),8)
  ack=json.loads((out/f'ack-{n}.json').read_text());(out/f'timing-{n}.json').write_text(json.dumps({'start':started,'done':time.monotonic(),'action':value}));print(out.name,value,ack,flush=True)
  if value['kind']=='capture' and 'error' not in ack:Image.frombytes('RGBA',(ack['width'],ack['height']),(out/f'frame-{n}.rgba').read_bytes()).save(out/f'provider-capture-{n}.png')
  time.sleep(.18);return ack
 for repeat in range(2):
  for down in [True,False]:
   when=time.monotonic();db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyKeyboardKeysym','(ub)',(97,down))
   with (R/'direct-inputs.jsonl').open('a') as f:f.write(json.dumps({'repeat':repeat,'keysym':97,'down':down,'time':when,'source':'direct real Mutter compositor session, no production provider'})+'\n')
   time.sleep(.25)
 time.sleep(.5);screenshot('after-direct-native-keys')
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
