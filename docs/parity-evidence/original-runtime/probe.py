import os,ctypes as C,time,subprocess,pathlib,json,socket,hashlib
from PIL import Image
p=pathlib.Path('/work/reports'); x=C.CDLL('libX11.so.6'); x.XOpenDisplay.restype=C.c_void_p; x.XDefaultRootWindow.argtypes=[C.c_void_p]; x.XDefaultRootWindow.restype=C.c_ulong
x.XFetchName.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_char_p)]; x.XQueryTree.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_ulong),C.POINTER(C.c_ulong),C.POINTER(C.POINTER(C.c_ulong)),C.POINTER(C.c_uint)]
x.XGetImage.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint,C.c_ulong,C.c_int]; x.XGetImage.restype=C.c_void_p
x.XGetPixel.argtypes=[C.c_void_p,C.c_int,C.c_int]; x.XGetPixel.restype=C.c_ulong; x.XDestroyImage.argtypes=[C.c_void_p]
error_cb=C.CFUNCTYPE(C.c_int,C.c_void_p,C.c_void_p)(lambda *_:0)
x.XSetErrorHandler(error_cb)
class Attr(C.Structure):
 _fields_=[('x',C.c_int),('y',C.c_int),('width',C.c_int),('height',C.c_int),('border_width',C.c_int),('depth',C.c_int),('visual',C.c_void_p),('root',C.c_ulong),('class_',C.c_int),('bit_gravity',C.c_int),('win_gravity',C.c_int),('backing_store',C.c_int),('backing_planes',C.c_ulong),('backing_pixel',C.c_ulong),('save_under',C.c_int),('colormap',C.c_ulong),('map_installed',C.c_int),('map_state',C.c_int),('all_event_masks',C.c_long),('your_event_mask',C.c_long),('do_not_propagate_mask',C.c_long),('override_redirect',C.c_int),('screen',C.c_void_p)]
x.XGetWindowAttributes.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(Attr)]
def capture(tag):
 d=x.XOpenDisplay(b':0'); assert d; r=x.XDefaultRootWindow(d); a=Attr(); x.XGetWindowAttributes(d,r,C.byref(a))
 wins=[]
 def walk(w):
  name=C.c_char_p(); x.XFetchName(d,w,C.byref(name)); aa=Attr(); x.XGetWindowAttributes(d,w,C.byref(aa))
  if name.value or aa.map_state==2: wins.append({'id':w,'name':name.value.decode(errors='replace') if name.value else '','mapped':aa.map_state,'size':[aa.width,aa.height], 'pos':[aa.x,aa.y], 'class':aa.class_})
  rr=C.c_ulong(); parent=C.c_ulong(); kids=C.POINTER(C.c_ulong)(); n=C.c_uint(); x.XQueryTree(d,w,C.byref(rr),C.byref(parent),C.byref(kids),C.byref(n))
  for i in range(n.value):walk(kids[i])
 walk(r)
 for win in wins:
  if win['mapped']!=2 or win['class']==2 or min(win['size'])<2:continue
  w,h=win['size']; im=x.XGetImage(d,win['id'],0,0,w,h,0xffffffff,2)
  if not im:continue
  raw=bytearray(w*h*3)
  for yy in range(h):
   for xx in range(w):
    v=x.XGetPixel(im,xx,yy); i=(yy*w+xx)*3; raw[i:i+3]=bytes(((v>>16)&255,(v>>8)&255,v&255))
  Image.frombytes('RGB',(w,h),bytes(raw)).save(p/(tag+'-'+str(win['id'])+'.png')); x.XDestroyImage(im)
 (p/(tag+'.json')).write_text(json.dumps(wins,indent=2)); print(json.dumps(wins),flush=True)
 return wins
assert hashlib.sha256(pathlib.Path('/work/original/MobaXterm_Personal_26.6.exe').read_bytes()).hexdigest()=='e9a66144b3a1c4d32f8062b1eb34e26e9ed5298106abf1982b4654ab107f13a5'
(p/'isolation.json').write_text(json.dumps({'home':os.environ['HOME'],'net':os.readlink('/proc/self/ns/net'),'display':os.environ.get('DISPLAY'),'host_home_visible':pathlib.Path('/home/asad').exists(),'run_visible':pathlib.Path('/run/user').exists()},indent=2))
# Keep the private initial X11 connection open, as the existing fixture does.
sock=socket.socket(socket.AF_UNIX); sock.connect('/tmp/.X11-unix/X0'); sock.sendall(b'l\0\x0b'+b'\0'*9); print('xready',sock.recv(8),flush=True)
(p/'environment.json').write_text(json.dumps(dict(os.environ),indent=2))
(p/'processes.txt').write_text('\n'.join(str(t)+': '+(t/'cmdline').read_bytes().replace(b'\0',b' ').decode(errors='replace') for t in pathlib.Path('/proc').iterdir() if t.name.isdigit()))
with open(p/'wine.log','w') as log:
 q=subprocess.Popen(['/work/root/usr/bin/wine','/work/original/MobaXterm_Personal_26.6.exe'],stdout=log,stderr=log)
 try:
  time.sleep(40); capture('initial')
  if pathlib.Path('/work/action.py').exists():exec(pathlib.Path('/work/action.py').read_text())
 finally:
  subprocess.run(['/work/root/usr/bin/wineserver','-k'],timeout=10); q.wait(timeout=10)
