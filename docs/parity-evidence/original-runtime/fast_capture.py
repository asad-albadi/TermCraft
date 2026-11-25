class XImg(C.Structure):
 _fields_=[('width',C.c_int),('height',C.c_int),('xoffset',C.c_int),('format',C.c_int),('data',C.c_void_p),('byte_order',C.c_int),('bitmap_unit',C.c_int),('bitmap_bit_order',C.c_int),('bitmap_pad',C.c_int),('depth',C.c_int),('bytes_per_line',C.c_int),('bits_per_pixel',C.c_int)]
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
  info=C.cast(im,C.POINTER(XImg)).contents
  if info.bits_per_pixel==32 and info.byte_order==0:
   raw=C.string_at(info.data,info.bytes_per_line*h)
   Image.frombytes('RGB',(w,h),raw,'raw','BGRX',info.bytes_per_line,1).save(p/(tag+'-'+str(win['id'])+'.png'))
  x.XDestroyImage(im)
 (p/(tag+'.json')).write_text(json.dumps(wins,indent=2)); print(json.dumps(wins),flush=True)
 return wins
