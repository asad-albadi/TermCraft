from gi.repository import Gio,GLib
import socket,threading,struct,fcntl
from PIL import Image
import os,sys,pathlib,subprocess,time,json,signal,shutil,ctypes
ctypes.CDLL(None).prctl(36,1,0,0,0)
B=pathlib.Path(__file__).resolve().parent;APP=B.parents[2];BUILD_RESULT=json.loads((APP/'docs/parity-evidence/parallel-integration-tigervnc-clipboard-fix-release/release-build-result.json').read_text());PACKAGE=pathlib.Path(BUILD_RESULT['package_root']);R=B/'gui-reports';R.mkdir(exist_ok=False);owned=[]
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
 compositor=start(['mutter','--headless','--wayland','--no-x11','--virtual-monitor','1600x1000','--debug-control','--mutter-plugin='+str(B/'tools/usr/lib/mutter-51/plugins/libdefault.so')],'mutter')
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
 helper=subprocess.Popen(['/bin/sh','-c','exec 3<&"$1" 4>&"$2"; exec "$3" "$4" "$5"','owned-fixture',str(vfd),str(zfd),str(PACKAGE/'termcraft-wayland-engine'),str(node['id']),str(node['info']['props']['object.serial'])],pass_fds=(vfd,zfd),stdout=open(R/'fixture-helper.log','wb'),stderr=subprocess.STDOUT);owned.append(('fixture-helper',helper));os.close(vfd);os.close(zfd);z.close();v.close()
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
  wait(lambda:len(latest)==3,8)
  if helper.poll() is not None:raise RuntimeError('native fixture helper ended; refusing stale screenshot')
  w,h,data=latest.copy();Image.frombytes('RGBA',(w,h),data).save(R/(name+'.png'))
 def pixel(x,y):
  if not latest:return None
  w,h,data=latest.copy();return tuple(data[(y*w+x)*4:(y*w+x)*4+4])
 # Establish an owned fixture keyboard before mapping the receiver, modelling an already-present desktop keyboard.
 for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyKeyboardKeysym','(ub)',(65505,down));time.sleep(.1)
 (R/'fixture-keyboard-primer.json').write_text(json.dumps({'keysym':65505,'events':['down','up'],'time':time.monotonic(),'before_target_map':True,'production_provider_not_started':True}))

 import hashlib,importlib.util
 assert hashlib.sha256((PACKAGE/'harbor-workspace').read_bytes()).hexdigest()==BUILD_RESULT['application_sha256']
 assert hashlib.sha256((PACKAGE/'termcraft-wayland-engine').read_bytes()).hexdigest()==BUILD_RESULT['wayland_helper_sha256']
 (R/'genuine-inputs.json').write_text(json.dumps(BUILD_RESULT,indent=2))
 spec=importlib.util.spec_from_file_location('startup',APP/'scripts/package-check-startup.py');startup=importlib.util.module_from_spec(spec);spec.loader.exec_module(startup)
 env=os.environ.copy();iso=pathlib.Path('/tmp/normal-startup');iso.mkdir();probe=R/'normal-startup';probe.mkdir()
 helpout=startup.run([PACKAGE/'harbor-workspace','--help'],iso,env,probe/'help.log');assert b'TermCraft remote workspace' in helpout
 handshake=startup.run([PACKAGE/'termcraft-rdp-engine'],iso,env,probe/'rdp-helper.log');assert handshake[0]==1;end=5+int.from_bytes(handshake[1:5],'big');assert json.loads(handshake[5:end])=={'Ready':{'version':1}}
 startup.rdp_session_probe(PACKAGE/'harbor-workspace',BUILD_RESULT['rdp_helper_sha256'],iso,env,probe)
 (probe/'result.json').write_text(json.dumps({'status':'PASS','normal_app_help':True,'actual_normal_app_RDP_session_TPKT':True,'actual_child_memfd_seals_hash':True,'preserved_workspace':True,'app_sha256':BUILD_RESULT['application_sha256'],'helper_sha256':BUILD_RESULT['rdp_helper_sha256']}))
 workspace=pathlib.Path('/tmp/tiger-gui-workspace');workspace.mkdir()
 profile={'id':'owned-tiger-vnc-gui','name':'Owned TigerVNC GUI qualification','protocol':'VNC','host':'127.0.0.1','port':5907,'user':'qualify','vnc_options':{'embedded':True,'allow_unencrypted':True,'security':'Password','clipboard':'Bidirectional','keyboard':'Remote'}}
 (workspace/'workspace.json').write_text(json.dumps({'sessions':[profile]}));(R/'nonsecret-session-profile.json').write_text(json.dumps(profile,indent=2))
 application=start([PACKAGE/'harbor-workspace','--data-dir',workspace,'--session','owned-tiger-vnc-gui'],'genuine-vnc-gui');time.sleep(3);screenshot('normal-vnc-authentication')
 secret=(APP/'target/agent-work/independent-protocol-daemons/secret').read_text().strip();assert len(secret)==8
 def fixture_click(x,y):
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(-10000.,-10000.));time.sleep(.1)
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(float(x),float(y)));time.sleep(.1)
  for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerButton','(ib)',(272,down));time.sleep(.1)
 def key(sym,down):db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyKeyboardKeysym','(ub)',(sym,down))
 def press(sym):key(sym,True);time.sleep(.07);key(sym,False);time.sleep(.09)
 def combo(mod,sym):key(mod,True);press(sym);key(mod,False);time.sleep(.2)
 commands=R/'commands';commands.mkdir();print('READY GENUINE NORMAL VNC GUI',flush=True)
 for idx in range(1000):
  path=commands/f'{idx}.json';wait(lambda:path.exists(),1200);c=json.loads(path.read_text());kind=c['kind'];ack={'ok':True,'time':time.monotonic()}
  try:
   if kind=='click':fixture_click(c['x'],c['y'])
   elif kind=='key':press(c['sym'])
   elif kind=='combo':combo(c['mod'],c['sym'])
   elif kind=='secret':
    for ch in secret:press(ord(ch))
   elif kind=='type':
    for ch in c['text']:press(ord(ch))
   elif kind=='scroll':db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerAxisDiscrete','(ui)',(0,c['steps']))
   elif kind=='peer':start(['python','-I',B/'clipboard-peer.py',R/'peer'],'owned-clipboard-peer')
   elif kind=='inspect':
    ack['app_alive']=application.poll() is None;ack['compositor_alive']=compositor.poll() is None
    hits=[];scanned=0
    for private in ['/tmp/real-home','/tmp/real-config','/tmp/real-data','/tmp/tiger-gui-workspace']:
     for p in pathlib.Path(private).rglob('*'):
      if p.is_file():
       scanned+=1
       if secret.encode() in p.read_bytes():hits.append(str(p))
    ack['password_persistence_scan']={'files':scanned,'hits':hits};assert not hits
    (R/(c['name']+'-processes.txt')).write_text(cmd(['ps','-eo','pid,ppid,comm,args']))
   elif kind=='finish':break
   else:raise RuntimeError('unknown action '+kind)
   time.sleep(c.get('delay',.4));screenshot(c.get('name','ui-'+str(idx)))
  except Exception as e:ack={'error':repr(e)}
  (commands/f'{idx}.ack').write_text(json.dumps(ack));print(idx,kind,ack,flush=True)
 secret=''
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
