import pathlib,sys,os,json,subprocess,time,socket,threading,select,hashlib
from Xlib import ext
ext.__extensions__=[e for e in ext.__extensions__ if e[0]=="XTEST"]
B=pathlib.Path(__file__).resolve().parent;P=pathlib.Path(sys.argv[1]); mode=sys.argv[2];R=B/mode;R.mkdir(exist_ok=True)
for d in ['home','config','data','drive']: (R/d).mkdir(exist_ok=True)
os.environ.update(HOME=str(R/'home'),XDG_CONFIG_HOME=str(R/'config'),XDG_DATA_HOME=str(R/'data'),WINIT_UNIX_BACKEND='x11',DISPLAY=':0')
import fcntl,struct
ctl=socket.socket();fcntl.ioctl(ctl,0x8914,struct.pack('16sH14s',b'lo',0x49,b''));ctl.close()
options={'security':'Tls' if mode=='tls-preflight' else 'Nla','clipboard':'Bidirectional','drives':[{'name':'OWNED','path':str(R/'drive'),'read_only':False}]}
(R/'data/workspace.json').write_text(json.dumps({'sessions':[{'id':'owned-rdp','protocol':'RDP','host':'127.0.0.1','port':3389,'user':'rdpowned','name':'Independent deployed RDP','rdp_options':options}]}))
os.chdir(B)
def shot(path):
 from Xlib import display,X
 from PIL import Image
 d=display.Display(':0');r=d.screen().root;g=r.get_geometry();z=r.get_image(0,0,g.width,g.height,X.ZPixmap,0xffffffff);Image.frombytes('RGB',(g.width,g.height),z.data,'raw','BGRX').save(path);d.close()
# Isolated-network local listener has only an owned AF_UNIX transport to the --network=none container.
def bridge(a):
 b=socket.socket(socket.AF_UNIX);b.connect('shadow.sock' if mode.startswith('shadow') else 'rdp.sock')
 try:
  while True:
   for x in select.select([a,b],[],[],10)[0]:
    data=x.recv(65536)
    if not data:return
    (b if x is a else a).sendall(data)
 finally:a.close();b.close()
s=socket.socket();s.bind(('127.0.0.1',3389));s.listen()
def serve():
 while True:
  a,_=s.accept();threading.Thread(target=bridge,args=(a,),daemon=True).start()
threading.Thread(target=serve,daemon=True).start()
appenv=os.environ.copy();appenv.pop('WAYLAND_DISPLAY',None);appenv.pop('WAYLAND_SOCKET',None)
p=subprocess.Popen([str(P/'harbor-workspace'),'--data-dir',str(R/'data'),'--session','owned-rdp'],stdout=open(R/'app.log','wb'),stderr=subprocess.STDOUT,env=appenv)
try:
 time.sleep(4);shot(R/'app.png')
 (R/'identity.json').write_text(json.dumps({'argv':[str(P/'harbor-workspace'),'--data-dir',str(R/'data'),'--session','owned-rdp'],'application_sha256':hashlib.sha256((P/'harbor-workspace').read_bytes()).hexdigest(),'engine_sha256':hashlib.sha256((P/'termcraft-rdp-engine').read_bytes()).hexdigest(),'app_alive':p.poll() is None,'netns':os.readlink('/proc/self/ns/net'),'scope':mode},indent=2))
 if mode.startswith('shadow'):
  from Xlib import display,X,XK,ext
  ext.__extensions__=[e for e in ext.__extensions__ if e[0]=="XTEST"]
  from Xlib.ext import xtest
  d=display.Display(':0')
  for w in d.screen().root.query_tree().children:
   if w.get_attributes().map_state==X.IsViewable:w.set_input_focus(X.RevertToParent,X.CurrentTime)
  d.sync()
  for n in range(100):
   f=R/f'cmd-{n}.json'
   deadline=time.monotonic()+140
   while not f.exists() and time.monotonic()<deadline:time.sleep(.05)
   if not f.exists():break
   c=json.loads(f.read_text())
   if c['kind']=='stop':break
   if c['kind']=='click':
    xtest.fake_input(d,X.MotionNotify,x=c['x'],y=c['y'],root=d.screen().root.id);d.sync();time.sleep(.15);xtest.fake_input(d,X.ButtonPress,1);d.sync();time.sleep(.15);xtest.fake_input(d,X.ButtonRelease,1)
   if c['kind']=='key':
    k=d.keysym_to_keycode(XK.string_to_keysym(c['key']));xtest.fake_input(d,X.KeyPress,k);xtest.fake_input(d,X.KeyRelease,k)
   if c['kind'] in ['text','password']:
    text=(B/'synthetic-password.private').read_text() if c['kind']=='password' else c['text']
    for ch in text:
     ks=ord(ch);k=d.keysym_to_keycode(ks)
     shift=ch.isupper() or ch in '_-!@#$%^&*()'
     if shift and ch!='-':xtest.fake_input(d,X.KeyPress,d.keysym_to_keycode(XK.XK_Shift_L))
     xtest.fake_input(d,X.KeyPress,k);xtest.fake_input(d,X.KeyRelease,k)
     if shift and ch!='-':xtest.fake_input(d,X.KeyRelease,d.keysym_to_keycode(XK.XK_Shift_L))
   d.sync();time.sleep(c.get('delay',1));shot(R/f'action-{n}.png');(R/f'ack-{n}').write_text('done')
finally:
 p.terminate();p.wait(timeout=5);(R/'cleanup.json').write_text(json.dumps({'app_pid':p.pid,'exit':p.returncode}))
