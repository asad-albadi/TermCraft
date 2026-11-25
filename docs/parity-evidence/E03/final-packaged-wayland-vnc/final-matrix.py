from gi.repository import Gio,GLib
import socket,threading,struct,fcntl
from PIL import Image
import os,sys,pathlib,subprocess,time,json,signal,shutil,ctypes
ctypes.CDLL(None).prctl(36,1,0,0,0)
B=pathlib.Path(__file__).resolve().parent;APP=B.parents[2];BUILD_RESULT=json.loads((APP/'docs/parity-evidence/parallel-integration-wayland-idle-fix-release/release-build-result.json').read_text());PACKAGE=pathlib.Path(BUILD_RESULT['package_root']);R=B/'final-reports';R.mkdir(exist_ok=False);owned=[]
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
 target_env=os.environ.copy();target_env['WAYLAND_DEBUG']='client'
 target=start(['python','-I',B/'owned-target.py',R/'target'],'owned-target',target_env);wait(lambda:(R/'target/events.jsonl').exists(),12);time.sleep(2);time.sleep(.5);screenshot('owned-target')
 wait(lambda:'wl_keyboard' in (R/'owned-target.log').read_text() and '.enter(' in (R/'owned-target.log').read_text(),5)
 (R/'keyboard-ready-before-provider.json').write_text(json.dumps({'time':time.monotonic(),'wire_lines':[l for l in (R/'owned-target.log').read_text().splitlines() if 'wl_keyboard' in l or ('wl_seat' in l and 'capabilities' in l)]}))
 def fixture_click(x,y):
  with (R/'fixture-consent-actions.jsonl').open('a') as f:f.write(json.dumps({'x':x,'y':y,'time':time.monotonic(),'purpose':'owned app/consent UI only'})+'\n')
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(-10000.,-10000.));time.sleep(.1)
  db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerMotionRelative','(dd)',(float(x),float(y)));time.sleep(.1)
  for down in [True,False]:db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerButton','(ib)',(272,down));time.sleep(.1)

 crypto=ctypes.CDLL("libcrypto.so")
 import secrets,hashlib
 secret=''.join(secrets.choice('abcdefghijklmnopqrstuvwxyz0123456789') for _ in range(8))
 assert hashlib.sha256((PACKAGE/'harbor-workspace').read_bytes()).hexdigest()==BUILD_RESULT['application_sha256']
 assert hashlib.sha256((PACKAGE/'termcraft-wayland-engine').read_bytes()).hexdigest()==BUILD_RESULT['wayland_helper_sha256']
 (R/'genuine-inputs.json').write_text(json.dumps(BUILD_RESULT,indent=2))
 appenv=os.environ.copy();appenv.pop('WAYLAND_DEBUG',None)
 application=start([PACKAGE/'harbor-workspace'],'genuine-packaged-app',appenv);time.sleep(3);screenshot('initial-app')
 def key(sym,down):db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyKeyboardKeysym','(ub)',(sym,down))
 def press(sym):key(sym,True);time.sleep(.07);key(sym,False);time.sleep(.09)
 def combo(mod,sym):key(mod,True);press(sym);key(mod,False);time.sleep(.2)
 class RFB:
  def __init__(self):
   self.s=socket.create_connection(('127.0.0.1',5900),timeout=5);self.s.settimeout(8);assert self.take(12)==b'RFB 003.008\n';self.s.sendall(b'RFB 003.008\n');n=self.take(1)[0];assert 2 in self.take(n);self.s.sendall(b'\x02');challenge=self.take(16);k=bytes(int(f'{v:08b}'[::-1],2) for v in secret.encode().ljust(8,b'\0'));schedule=ctypes.create_string_buffer(128);crypto.DES_set_key_unchecked(ctypes.c_char_p(k),schedule);encrypted=ctypes.create_string_buffer(16);crypto.DES_ecb_encrypt(ctypes.c_char_p(challenge[:8]),ctypes.byref(encrypted,0),schedule,1);crypto.DES_ecb_encrypt(ctypes.c_char_p(challenge[8:]),ctypes.byref(encrypted,8),schedule,1);self.s.sendall(encrypted.raw);assert self.take(4)==b'\0'*4;self.s.sendall(b'\x01');h=self.take(24);self.w,self.h=struct.unpack('>HH',h[:4]);self.name=self.take(struct.unpack('>I',h[20:])[0]);self.s.sendall(struct.pack('>BBH',2,0,1)+struct.pack('>i',0))
  def take(self,n):
   data=b''
   while len(data)<n:
    x=self.s.recv(n-len(data))
    if not x:raise EOFError('RFB ended')
    data+=x
   return data
  def capture(self,path):
   self.s.sendall(struct.pack('>BBHHHH',3,0,0,0,self.w,self.h));assert self.take(1)==b'\0';n=struct.unpack('>xH',self.take(3))[0];image=Image.new('RGB',(self.w,self.h))
   for i in range(n):
    x,y,w,h,enc=struct.unpack('>HHHHi',self.take(12));assert enc==0;raw=self.take(w*h*4);image.paste(Image.frombytes('RGB',(w,h),raw,'raw','BGRX'),(x,y))
   image.save(path);return {'width':self.w,'height':self.h,'server_name':self.name.decode(),'pixels_sha256':hashlib.sha256(image.tobytes()).hexdigest(),'rfb_artifact':str(path)}
  def key(self,sym,down):self.s.sendall(struct.pack('>BBHI',4,int(down),0,sym))
  def pointer(self,buttons,x,y):self.s.sendall(struct.pack('>BBHH',5,buttons,x,y))
 def product_helpers():
  result=[]
  for proc in pathlib.Path('/proc').iterdir():
   if not proc.name.isdigit():continue
   try:
    status=(proc/'status').read_text();ppid=int(next(l.split()[1] for l in status.splitlines() if l.startswith('PPid:')))
    args=(proc/'cmdline').read_bytes().replace(b'\0',b' ').decode(errors='replace')
    if ppid==application.pid and args.startswith('termcraft-wayland-engine '):result.append({'pid':int(proc.name),'ppid':ppid,'argv':args,'exe':os.readlink(proc/'exe'),'sha256':hashlib.sha256((proc/'exe').read_bytes()).hexdigest()})
   except (FileNotFoundError,ProcessLookupError,PermissionError):pass
  return result
 def listening():return '127.0.0.1:5900' in cmd(['ss','-ltnH'])
 def closed_sessions():return len(__import__('re').findall(r'(?:remote desktop|screen cast) session owned by .* closed',(R/'portal.log').read_text()))
 rfb=None;commands=R/'commands';commands.mkdir();print('READY REAL GUI',flush=True)
 for idx in range(1000):
  path=commands/f'{idx}.json';wait(lambda:path.exists(),1200);c=json.loads(path.read_text());kind=c['kind'];ack={'ok':True,'time':time.monotonic()}
  try:
   if kind=='click':fixture_click(c['x'],c['y'])
   elif kind=='key':press(c['sym'])
   elif kind=='combo':combo(c['mod'],c['sym'])
   elif kind=='type':
    for ch in c['text']:press(ord(ch))
   elif kind=='secret':
    for ch in secret:press(ord(ch))
   elif kind=='scroll':
    db('org.gnome.Mutter.RemoteDesktop',rdpath,'org.gnome.Mutter.RemoteDesktop.Session','NotifyPointerAxisDiscrete','(ui)',(0,c['steps']))
   elif kind=='rfb_connect':rfb=RFB();ack['authenticated']=True;ack['dimensions']=[rfb.w,rfb.h]
   elif kind=='rfb_capture':ack.update(rfb.capture(R/(c['name']+'.rfb.png')))
   elif kind=='rfb_key':rfb.key(c['sym'],c['down'])
   elif kind=='rfb_pointer':rfb.pointer(c['buttons'],c['x'],c['y'])
   elif kind=='rfb_disconnect':rfb.s.close();rfb=None
   elif kind=='idle':
    before=time.monotonic();time.sleep(c['seconds']);ack.update({'idle_seconds':time.monotonic()-before,'native_observer_alive':helper.poll() is None,'listener_alive':listening(),'product_helpers':product_helpers()})
   elif kind=='rfb_probe_closed':
    rfb.s.settimeout(2)
    try:ack['eof']=rfb.s.recv(1)==b''
    except (ConnectionResetError,BrokenPipeError):ack['eof']=True
   elif kind=='click_and_wait_closed':
    ack.update({'pre_listener':listening(),'pre_helpers':product_helpers(),'pre_closed_sessions':closed_sessions()});before=time.monotonic();fixture_click(c['x'],c['y']);wait(lambda:not listening() and not product_helpers(),6);ack.update({'cleanup_seconds':time.monotonic()-before,'post_listener':listening(),'post_helpers':product_helpers(),'post_closed_sessions':closed_sessions()})
   elif kind=='revoke':
    import re,xml.etree.ElementTree as ET
    owner=re.findall(r"remote desktop session owned by '(:[0-9.]+)' started",(R/'portal.log').read_text())[-1].lstrip(':').replace('.','_')
    base='/org/freedesktop/portal/desktop/session/'+owner;backend='org.freedesktop.impl.portal.desktop.gnome'
    xml=db(backend,base,'org.freedesktop.DBus.Introspectable','Introspect')[0];(R/'revoke-session-introspection.xml').write_text(xml);children=ET.fromstring(xml).findall('node');assert len(children)==1
    session=base+'/'+children[0].attrib['name'];before=time.monotonic();ack.update({'session':session,'pre_listener':listening(),'pre_helpers':product_helpers()});db(backend,session,'org.freedesktop.impl.portal.Session','Close');wait(lambda:not listening() and not product_helpers(),6);ack.update({'cleanup_seconds':time.monotonic()-before,'post_listener':listening(),'post_helpers':product_helpers(),'closed_sessions':closed_sessions()})
   elif kind=='inspect':
    ack['app_alive']=application.poll() is None;ack['target_alive']=target.poll() is None;ack['compositor_alive']=compositor.poll() is None;ack['product_helpers']=product_helpers();ack['listener']=listening();ack['closed_sessions']=closed_sessions()
    scanned=0;hits=[]
    for private_root in ['/tmp/real-home','/tmp/real-config','/tmp/real-data']:
     for file in pathlib.Path(private_root).rglob('*'):
      if file.is_file():
       scanned+=1
       if secret.encode() in file.read_bytes():hits.append(str(file))
    ack['private_persistence_scan']={'files':scanned,'plaintext_password_hits':hits}
    assert not hits,'password persisted in private workspace/config/data'
    (R/(c['name']+'-processes.txt')).write_text(cmd(['ps','-eo','pid,ppid,comm,args']));(R/(c['name']+'-listeners.txt')).write_text(cmd(['ss','-ltnp']))
   elif kind=='finish':break
   else:raise RuntimeError('unknown command '+kind)
   time.sleep(c.get('delay',.4));screenshot(c.get('name','ui-'+str(idx)))
  except Exception as e:ack={'error':repr(e)}
  (commands/f'{idx}.ack').write_text(json.dumps(ack));print(idx,kind,ack,flush=True)
 secret='';
 if rfb:rfb.s.close()
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
