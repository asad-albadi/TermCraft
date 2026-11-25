"""Private xdm setup client, executed with xdm's negotiated authorization."""
import ctypes
import os
import pathlib
import sys
import time
x11 = ctypes.CDLL("libX11.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XCreateSimpleWindow.argtypes = [ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_int,ctypes.c_uint,ctypes.c_uint,ctypes.c_uint,ctypes.c_ulong,ctypes.c_ulong]
x11.XCreateSimpleWindow.restype = ctypes.c_ulong
x11.XDefaultScreen.argtypes = [ctypes.c_void_p]
x11.XDisplayWidth.argtypes = [ctypes.c_void_p,ctypes.c_int]
x11.XDisplayHeight.argtypes = [ctypes.c_void_p,ctypes.c_int]
x11.XGetImage.argtypes = [ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_int,ctypes.c_uint,ctypes.c_uint,ctypes.c_ulong,ctypes.c_int]
x11.XGetImage.restype = ctypes.c_void_p
x11.XGetPixel.argtypes = [ctypes.c_void_p,ctypes.c_int,ctypes.c_int]
x11.XGetPixel.restype = ctypes.c_ulong
x11.XMapWindow.argtypes = [ctypes.c_void_p,ctypes.c_ulong]
x11.XSync.argtypes = [ctypes.c_void_p,ctypes.c_int]
display = x11.XOpenDisplay(None)
if not display:
    raise RuntimeError("The real display manager's negotiated X11 authorization failed")
window = x11.XCreateSimpleWindow(display,x11.XDefaultRootWindow(display),20,20,320,200,0,0,0x229944)
x11.XMapWindow(display,window)
x11.XSync(display,0)
screen = x11.XDefaultScreen(display)
image = x11.XGetImage(display,window,0,0,1,1,ctypes.c_ulong(-1),2)
if not image or x11.XGetPixel(image,0,0) != 0x229944:
    raise RuntimeError('Negotiated client window did not render its green reference pixel')
x11.XkbKeycodeToKeysym.argtypes = [ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_int, ctypes.c_int]
x11.XkbKeycodeToKeysym.restype = ctypes.c_ulong
keysyms = tuple(x11.XkbKeycodeToKeysym(display, code, 0, 0) for code in (29, 52))
pathlib.Path(sys.argv[1] + ".keyboard").write_text(" ".join(map(str, keysyms)))
pathlib.Path(sys.argv[1] + ".authority").write_text(os.environ["XAUTHORITY"])
pathlib.Path(sys.argv[1]).write_text(os.environ['DISPLAY'] + '; mapped window ' + str(window) + '; dimensions ' + str(x11.XDisplayWidth(display,screen)) + 'x' + str(x11.XDisplayHeight(display,screen)) + '; green pixel verified')
time.sleep(20)
