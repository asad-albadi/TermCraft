exec(pathlib.Path('/work/fast_capture.py').read_text())
d=x.XOpenDisplay(b':0'); t=C.CDLL('libXtst.so.6'); t.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]; t.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]; t.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]; x.XFlush.argtypes=[C.c_void_p]; x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong]; x.XKeysymToKeycode.restype=C.c_uint
def click(xx,yy):
 t.XTestFakeMotionEvent(d,-1,xx+offset_x,yy+offset_y,0);t.XTestFakeButtonEvent(d,1,1,0);t.XTestFakeButtonEvent(d,1,0,0);x.XFlush(d);time.sleep(1)
def key(sym):
 code=x.XKeysymToKeycode(d,sym);t.XTestFakeKeyEvent(d,code,1,0);t.XTestFakeKeyEvent(d,code,0,0);x.XFlush(d);time.sleep(.5)
main=next(w for w in json.loads((p/'initial.json').read_text()) if w['name']=='MobaXterm' and w['size'][0]>500 and w['mapped']==2)
x.XTranslateCoordinates.argtypes=[C.c_void_p,C.c_ulong,C.c_ulong,C.c_int,C.c_int,C.POINTER(C.c_int),C.POINTER(C.c_int),C.POINTER(C.c_ulong)]
ox=C.c_int();oy=C.c_int();child=C.c_ulong();x.XTranslateCoordinates(d,main['id'],x.XDefaultRootWindow(d),0,0,C.byref(ox),C.byref(oy),C.byref(child));offset_x=ox.value;offset_y=oy.value
click(345,10);capture('games-menu');key(0xff1b)
click(404,10);capture('settings-menu');key(0xff1b)
click(552,36);capture('settings-dialog')
files=[]
for f in pathlib.Path('/home/test/prefix/drive_c/users/test/AppData/Local/Temp/Mxt266').rglob('*'):
 if f.is_file():
  files.append(str(f))
  if f.name.lower() in ['mobapt','apt-get','apt-cyg']:
   (p/('original-script-'+f.name)).write_bytes(f.read_bytes())
(p/'runtime-file-catalog.json').write_text(json.dumps(files,indent=2))
deadline=time.monotonic()+35
while time.monotonic()<deadline:
 nxt=pathlib.Path('/work/next-action.py')
 if nxt.exists():
  data=nxt.read_text();nxt.unlink();exec(data)
 time.sleep(.1)
