"""Send a real X11 WM_DELETE_WINDOW client message to the one owned app window."""
import ctypes as c,sys
x=c.CDLL('libX11.so.6');x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
x.XInternAtom.argtypes=[c.c_void_p,c.c_char_p,c.c_int];x.XInternAtom.restype=c.c_ulong
x.XSendEvent.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_long,c.c_void_p];x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
class Data(c.Union):_fields_=[('b',c.c_char*20),('s',c.c_short*10),('l',c.c_long*5)]
class Client(c.Structure):_fields_=[('type',c.c_int),('serial',c.c_ulong),('send_event',c.c_int),('display',c.c_void_p),('window',c.c_ulong),('message_type',c.c_ulong),('format',c.c_int),('data',Data)]
class Event(c.Union):_fields_=[('client',Client),('pad',c.c_long*24)]
d=x.XOpenDisplay(None); assert d
w=int(sys.argv[1]); e=Event();e.client.type=33;e.client.send_event=1;e.client.display=d;e.client.window=w;e.client.message_type=x.XInternAtom(d,b'WM_PROTOCOLS',0);e.client.format=32;e.client.data.l[0]=x.XInternAtom(d,b'WM_DELETE_WINDOW',0)
assert x.XSendEvent(d,w,0,0,c.byref(e));x.XFlush(d);x.XCloseDisplay(d);print('Sent WM_DELETE_WINDOW to owned window',w)
