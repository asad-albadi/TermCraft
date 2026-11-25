"""Use XTest against the pinned xdm chooser, not a protocol substitute."""

import ctypes as C
import sys
import time

x11 = C.CDLL("libX11.so.6")
xtest = C.CDLL("libXtst.so.6")
Display = C.c_void_p
Window = C.c_ulong
Int = C.c_int
UInt = C.c_uint
P = C.POINTER


def bind(lib, name, result, *args):
    fn = getattr(lib, name)
    fn.restype = result
    fn.argtypes = args
    return fn


open_display = bind(x11, "XOpenDisplay", Display, C.c_char_p)
close_display = bind(x11, "XCloseDisplay", Int, Display)
root_window = bind(x11, "XDefaultRootWindow", Window, Display)
query_tree = bind(x11, "XQueryTree", Int, Display, Window, P(Window), P(Window), P(P(Window)), P(UInt))
window_title = bind(x11, "XFetchName", Int, Display, Window, P(C.c_char_p))
translate = bind(x11, "XTranslateCoordinates", Int, Display, Window, Window, Int, Int, P(Int), P(Int), P(Window))
free = bind(x11, "XFree", Int, C.c_void_p)
flush = bind(x11, "XFlush", Int, Display)
motion = bind(xtest, "XTestFakeMotionEvent", Int, Display, Int, Int, Int, C.c_ulong)
button = bind(xtest, "XTestFakeButtonEvent", Int, Display, UInt, Int, C.c_ulong)
get_image = bind(x11, "XGetImage", C.c_void_p, Display, Window, Int, Int, UInt, UInt, C.c_ulong, Int)
get_pixel = bind(x11, "XGetPixel", C.c_ulong, C.c_void_p, Int, Int)
destroy_image = bind(x11, "XDestroyImage", Int, C.c_void_p)


def wait(probe, seconds, expectation):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if value := probe():
            return value
        time.sleep(0.05)
    raise RuntimeError(expectation)


def children(display, window):
    root, parent, nodes, count = Window(), Window(), P(Window)(), UInt()
    assert query_tree(display, window, C.byref(root), C.byref(parent), C.byref(nodes), C.byref(count))
    try:
        return [nodes[index] for index in range(count.value)]
    finally:
        if nodes:
            free(nodes)


def title(display, window):
    value = C.c_char_p()
    if window_title(display, window, C.byref(value)):
        try:
            return value.value
        finally:
            free(value)
    return None


def populated(display, window):
    # Client-side scan of the single Xaw host row. An empty list area is flat.
    image = get_image(display, window, 8, 3, 80, 12, C.c_ulong(-1), 2)
    assert image
    try:
        background = get_pixel(image, 0, 0)
        return any(get_pixel(image, col, row) != background for row in range(12) for col in range(80))
    finally:
        destroy_image(image)


assert sys.argv[1] in ("observe", "accept")
display = wait(lambda: open_display(None), 6, "chooser authentication did not complete")
try:
    root = root_window(display)
    chooser = wait(
        lambda: next((win for win in children(display, root) if title(display, win) == b"chooser"), None),
        5,
        "real xdm chooser window did not appear",
    )
    print("Authenticated to mapped real xdm chooser", flush=True)
    if sys.argv[1] == "accept":
        # The pinned Chooser defaults use Paned -> Viewport -> Clip -> List.
        paned, = children(display, chooser)
        _, viewport, _ = children(display, paned)
        clip, = children(display, viewport)
        choice, = children(display, clip)
        wait(lambda: populated(display, choice), 5, "chooser did not render its single host row")
        x, y, child = Int(), Int(), Window()
        assert translate(display, choice, root, 30, 10, C.byref(x), C.byref(y), C.byref(child))
        assert motion(display, 0, x.value, y.value, 0)
        for _ in range(2):
            assert button(display, 1, 1, 0)
            assert button(display, 1, 0, 0)
        flush(display)
finally:
    close_display(display)
