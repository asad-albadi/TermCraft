"""Read one active RandR output from the existing private Cage X display."""
import ctypes as c
x=c.CDLL('libX11.so.6');r=c.CDLL('libXrandr.so.2')
class Resources(c.Structure):
    _fields_=[('timestamp',c.c_ulong),('config_timestamp',c.c_ulong),('ncrtc',c.c_int),('crtcs',c.POINTER(c.c_ulong)),('noutput',c.c_int),('outputs',c.POINTER(c.c_ulong)),('nmode',c.c_int),('modes',c.c_void_p)]
class Output(c.Structure):
    _fields_=[('timestamp',c.c_ulong),('crtc',c.c_ulong),('name',c.c_void_p),('name_len',c.c_int),('mm_width',c.c_ulong),('mm_height',c.c_ulong),('connection',c.c_int),('subpixel_order',c.c_int),('ncrtc',c.c_int),('crtcs',c.POINTER(c.c_ulong)),('nclone',c.c_int),('clones',c.POINTER(c.c_ulong)),('nmode',c.c_int),('npreferred',c.c_int),('modes',c.POINTER(c.c_ulong))]
x.XOpenDisplay.restype=c.c_void_p;x.XDefaultRootWindow.argtypes=[c.c_void_p];x.XDefaultRootWindow.restype=c.c_ulong
x.XDefaultScreen.argtypes=[c.c_void_p];x.XDisplayWidth.argtypes=[c.c_void_p,c.c_int];x.XDisplayHeight.argtypes=[c.c_void_p,c.c_int]
r.XRRGetScreenResourcesCurrent.argtypes=[c.c_void_p,c.c_ulong];r.XRRGetScreenResourcesCurrent.restype=c.POINTER(Resources)
r.XRRGetOutputInfo.argtypes=[c.c_void_p,c.POINTER(Resources),c.c_ulong];r.XRRGetOutputInfo.restype=c.POINTER(Output)
d=x.XOpenDisplay(None)
if not d:raise RuntimeError('No private fixture display')
resources=r.XRRGetScreenResourcesCurrent(d,x.XDefaultRootWindow(d))
if not resources:raise RuntimeError('No RandR resources on private fixture')
for i in range(resources.contents.noutput):
    output=r.XRRGetOutputInfo(d,resources,resources.contents.outputs[i])
    if output and output.contents.crtc and output.contents.connection==0:
        name=c.string_at(output.contents.name,output.contents.name_len).decode('ascii')
        screen=x.XDefaultScreen(d)
        print(name,x.XDisplayWidth(d,screen),x.XDisplayHeight(d,screen));break
else:raise RuntimeError('No active private fixture output')
