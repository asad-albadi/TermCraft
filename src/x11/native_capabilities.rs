//! Linux native-library implementation. Called only in the disposable probe process.
//! All function pointers are resolved before use; a missing optional library is evidence,
//! never a successful capability. No native graphics library is linked on other targets.
use super::capabilities::{OpenGlEvidence, XCapabilities};
use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_int, c_uint, c_ulong, c_void},
    ptr,
    sync::atomic::{AtomicI32, Ordering},
};
type Display = c_void;
type Xid = c_ulong;
#[repr(C)]
struct VisualInfo {
    visual: *mut c_void,
    visualid: Xid,
    screen: c_int,
    depth: c_int,
    class: c_int,
    red_mask: c_ulong,
    green_mask: c_ulong,
    blue_mask: c_ulong,
    colormap_size: c_int,
    bits_per_rgb: c_int,
}
#[repr(C)]
struct WindowAttributes {
    background_pixmap: Xid,
    background_pixel: c_ulong,
    border_pixmap: Xid,
    border_pixel: c_ulong,
    bit_gravity: c_int,
    win_gravity: c_int,
    backing_store: c_int,
    backing_planes: c_ulong,
    backing_pixel: c_ulong,
    save_under: c_int,
    event_mask: std::ffi::c_long,
    do_not_propagate_mask: std::ffi::c_long,
    override_redirect: c_int,
    colormap: Xid,
    cursor: Xid,
}
#[repr(C)]
struct ScreenSize {
    width: c_int,
    height: c_int,
    mwidth: c_int,
    mheight: c_int,
}
#[repr(C)]
struct XErrorEvent {
    event_type: c_int,
    display: *mut Display,
    resourceid: Xid,
    serial: c_ulong,
    error_code: u8,
    request_code: u8,
    minor_code: u8,
}
static X_ERROR: AtomicI32 = AtomicI32::new(0);
unsafe extern "C" fn error_handler(_: *mut Display, event: *mut XErrorEvent) -> c_int {
    X_ERROR.store(unsafe { (*event).error_code } as i32, Ordering::Relaxed);
    0
}
macro_rules! sym {
    ($lib:expr, $name:literal, $ty:ty) => {
        *$lib
            .get::<$ty>(concat!($name, "\0").as_bytes())
            .map_err(|e| e.to_string())?
    };
}
macro_rules! call { ($lib:expr, $name:literal, ($($ty:ty),*) -> $ret:ty, $($arg:expr),* $(,)?) => { (sym!($lib, $name, unsafe extern "C" fn($($ty),*) -> $ret))($($arg),*) }; }
unsafe fn text(pointer: *const u8) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer.cast()) }
            .to_string_lossy()
            .into_owned()
    }
}

pub(super) unsafe fn probe() -> Result<XCapabilities, String> {
    unsafe {
        let x = Library::new("libX11.so.6").map_err(|e| format!("Native Xlib unavailable: {e}"))?;
        call!(&x, "XSetErrorHandler", (unsafe extern "C" fn(*mut Display,*mut XErrorEvent)->c_int) -> *mut c_void, error_handler);
        let d = call!(&x, "XOpenDisplay", (*const c_char) -> *mut Display, ptr::null());
        if d.is_null() {
            return Err("Cannot authenticate to the owned X display".into());
        }
        // Xlib stores extension close-display callbacks in these libraries.
        // Keep every loaded library alive until after XCloseDisplay.
        let gl = Library::new("libGL.so.1");
        let comp = Library::new("libXcomposite.so.1");
        let rr = Library::new("libXrandr.so.2");
        let mut report = XCapabilities::default();
        let root = call!(&x, "XDefaultRootWindow", (*mut Display) -> Xid, d);
        report.dimensions = geometry(&x, d, root)?;
        report.initial_mode_restored = true;
        if let Err(e) = gl
            .as_ref()
            .map_err(|e| e.to_string())
            .and_then(|lib| glx(&x, lib, d, root, &mut report))
        {
            report.errors.push(format!("OpenGL/GLX: {e}"));
        }
        X_ERROR.store(0, Ordering::Relaxed);
        if let Err(e) = comp
            .as_ref()
            .map_err(|e| e.to_string())
            .and_then(|lib| composite(&x, lib, d, root, &mut report))
        {
            report.errors.push(format!("Composite: {e}"));
        }
        X_ERROR.store(0, Ordering::Relaxed);
        if let Err(e) = rr
            .as_ref()
            .map_err(|e| e.to_string())
            .and_then(|lib| randr(&x, lib, d, root, &mut report))
        {
            report.errors.push(format!("RandR: {e}"));
        }
        call!(&x, "XCloseDisplay", (*mut Display) -> c_int, d);
        if !report.initial_mode_restored {
            return Err("Graphics probe could not restore the initial screen mode; discard this fresh owned display".into());
        }
        Ok(report)
    }
}
unsafe fn sync(x: &Library, d: *mut Display) -> Result<(), String> {
    unsafe {
        call!(x, "XSync", (*mut Display,c_int)->c_int, d, 0);
        let error = X_ERROR.swap(0, Ordering::Relaxed);
        if error == 0 {
            Ok(())
        } else {
            Err(format!("live X request failed with protocol error {error}"))
        }
    }
}
unsafe fn geometry(x: &Library, d: *mut Display, window: Xid) -> Result<[u32; 2], String> {
    unsafe {
        let (mut root, mut px, mut py, mut w, mut h, mut border, mut depth) = (0, 0, 0, 0, 0, 0, 0);
        let ok = call!(x,"XGetGeometry",(*mut Display,Xid,*mut Xid,*mut c_int,*mut c_int,*mut c_uint,*mut c_uint,*mut c_uint,*mut c_uint)->c_int,d,window,&mut root,&mut px,&mut py,&mut w,&mut h,&mut border,&mut depth);
        sync(x, d)?;
        if ok == 0 {
            return Err("Cannot query live drawable dimensions".into());
        }
        Ok([w, h])
    }
}
unsafe fn glx(
    x: &Library,
    gl: &Library,
    d: *mut Display,
    root: Xid,
    r: &mut XCapabilities,
) -> Result<(), String> {
    unsafe {
        let (mut major, mut minor) = (0, 0);
        if call!(&gl,"glXQueryVersion",(*mut Display,*mut c_int,*mut c_int)->c_int,d,&mut major,&mut minor)
            == 0
        {
            return Err("extension unavailable".into());
        }
        r.glx_version = Some(format!("{major}.{minor}"));
        let screen = call!(x,"XDefaultScreen",(*mut Display)->c_int,d);
        let mut attrs = [4, 5, 8, 8, 9, 8, 10, 8, 0]; // RGBA, doublebuffer, RGB >= 8
        let visual = call!(&gl,"glXChooseVisual",(*mut Display,c_int,*mut c_int)->*mut VisualInfo,d,screen,attrs.as_mut_ptr());
        if visual.is_null() {
            return Err("no suitable RGBA GLX visual".into());
        }
        let colormap = call!(x,"XCreateColormap",(*mut Display,Xid,*mut c_void,c_int)->Xid,d,root,(*visual).visual,0);
        let mut wa: WindowAttributes = std::mem::zeroed();
        wa.colormap = colormap;
        wa.border_pixel = 0;
        let window = call!(x,"XCreateWindow",(*mut Display,Xid,c_int,c_int,c_uint,c_uint,c_uint,c_int,c_uint,*mut c_void,c_ulong,*mut WindowAttributes)->Xid,d,root,0,0,64,64,0,(*visual).depth,1,(*visual).visual,(1<<13)|(1<<3),&mut wa);
        call!(x,"XMapWindow",(*mut Display,Xid)->c_int,d,window);
        sync(x, d)?;
        let context = call!(&gl,"glXCreateContext",(*mut Display,*mut VisualInfo,*mut c_void,c_int)->*mut c_void,d,visual,ptr::null_mut(),1);
        call!(x,"XFree",(*mut c_void)->c_int,visual.cast());
        if context.is_null() {
            call!(x,"XDestroyWindow",(*mut Display,Xid)->c_int,d,window);
            call!(x,"XFreeColormap",(*mut Display,Xid)->c_int,d,colormap);
            return Err(
                "GLX context unavailable (indirect GLX remains disabled by server policy)".into(),
            );
        }
        let current =
            call!(&gl,"glXMakeCurrent",(*mut Display,Xid,*mut c_void)->c_int,d,window,context);
        let render = (|| -> Result<OpenGlEvidence, String> {
            sync(x, d)?;
            if current == 0 {
                return Err("GLX context could not become current".into());
            }
            let string = sym!(
                &gl,
                "glGetString",
                unsafe extern "C" fn(c_uint) -> *const u8
            );
            let vendor = text(string(0x1f00));
            let renderer = text(string(0x1f01));
            let version = text(string(0x1f02));
            call!(&gl,"glViewport",(c_int,c_int,c_int,c_int)->(),0,0,64,64);
            call!(&gl,"glDrawBuffer",(c_uint)->(),0x0405); // back
            call!(&gl,"glEnable",(c_uint)->(),0x0c11); // scissor test
            let colors = [
                [1., 0., 0., 1.],
                [0., 1., 0., 1.],
                [0., 0., 1., 1.],
                [1., 1., 0., 1.],
            ];
            for (i, color) in colors.iter().enumerate() {
                call!(&gl,"glScissor",(c_int,c_int,c_int,c_int)->(),(i%2) as i32*32,(i/2) as i32*32,32,32);
                call!(&gl,"glClearColor",(f32,f32,f32,f32)->(),color[0],color[1],color[2],color[3]);
                call!(&gl,"glClear",(c_uint)->(),0x4000);
            }
            call!(&gl,"glFinish",()->(),);
            call!(&gl,"glReadBuffer",(c_uint)->(),0x0405);
            let mut pixels = vec![0u8; 64 * 64 * 4];
            call!(&gl,"glReadPixels",(c_int,c_int,c_int,c_int,c_uint,c_uint,*mut c_void)->(),0,0,64,64,0x1908,0x1401,pixels.as_mut_ptr().cast());
            let error = call!(&gl,"glGetError",()->c_uint,);
            if error != 0 {
                return Err(format!("OpenGL render/readback error {error:#x}"));
            }
            let mut matched = 0;
            for y in 0..64 {
                for px in 0..64 {
                    let expected = colors[(y / 32) * 2 + px / 32];
                    let actual = &pixels[(y * 64 + px) * 4..][..3];
                    if actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (*a as f32 - b * 255.).abs() <= 3.)
                    {
                        matched += 1;
                    }
                }
            }
            call!(&gl,"glXSwapBuffers",(*mut Display,Xid)->(),d,window);
            let lower = renderer.to_ascii_lowercase();
            use base64::Engine;
            use image::ImageEncoder;
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(&pixels, 64, 64, image::ExtendedColorType::Rgba8)
                .map_err(|error| format!("Could not encode actual GL readback: {error}"))?;
            let scene_png_base64 = base64::engine::general_purpose::STANDARD.encode(png);
            Ok(OpenGlEvidence {
                vendor,
                renderer,
                version,
                direct: call!(&gl,"glXIsDirect",(*mut Display,*mut c_void)->c_int,d,context) != 0,
                software: ["llvmpipe", "softpipe", "swrast", "software", "swr"]
                    .iter()
                    .any(|s| lower.contains(s)),
                matched_pixels: matched,
                sampled_pixels: 4096,
                scene_png_base64,
            })
        })();
        call!(&gl,"glXMakeCurrent",(*mut Display,Xid,*mut c_void)->c_int,d,0,ptr::null_mut());
        call!(&gl,"glXDestroyContext",(*mut Display,*mut c_void)->(),d,context);
        call!(x,"XDestroyWindow",(*mut Display,Xid)->c_int,d,window);
        call!(x,"XFreeColormap",(*mut Display,Xid)->c_int,d,colormap);
        r.opengl = Some(render?);
        if !r.rendering_verified() {
            return Err("known four-color GL scene failed tolerant pixel reference".into());
        }
        Ok(())
    }
}
unsafe fn composite(
    x: &Library,
    lib: &Library,
    d: *mut Display,
    root: Xid,
    r: &mut XCapabilities,
) -> Result<(), String> {
    unsafe {
        let (mut major, mut minor) = (0, 0);
        if call!(&lib,"XCompositeQueryVersion",(*mut Display,*mut c_int,*mut c_int)->c_int,d,&mut major,&mut minor)
            == 0
        {
            return Err("extension unavailable".into());
        }
        r.composite_version = Some(format!("{major}.{minor}"));
        let window = call!(x,"XCreateSimpleWindow",(*mut Display,Xid,c_int,c_int,c_uint,c_uint,c_uint,c_ulong,c_ulong)->Xid,d,root,0,0,32,32,0,0,0);
        call!(&lib,"XCompositeRedirectWindow",(*mut Display,Xid,c_int)->(),d,window,1);
        call!(x,"XMapWindow",(*mut Display,Xid)->c_int,d,window);
        sync(x, d)?;
        let pixmap = call!(&lib,"XCompositeNameWindowPixmap",(*mut Display,Xid)->Xid,d,window);
        let gc = call!(x,"XCreateGC",(*mut Display,Xid,c_ulong,*mut c_void)->*mut c_void,d,window,0,ptr::null_mut());
        call!(x,"XSetForeground",(*mut Display,*mut c_void,c_ulong)->c_int,d,gc,0x00ff00);
        call!(x,"XFillRectangle",(*mut Display,Xid,*mut c_void,c_int,c_int,c_uint,c_uint)->c_int,d,window,gc,0,0,32,32);
        sync(x, d)?;
        // Manual opaque composition: paint a blue output, copy the redirected
        // green window's offscreen pixmap into its left half, then read the output.
        let output = call!(x,"XCreateSimpleWindow",(*mut Display,Xid,c_int,c_int,c_uint,c_uint,c_uint,c_ulong,c_ulong)->Xid,d,root,80,0,64,32,0,0,0x0000ff);
        call!(x,"XMapWindow",(*mut Display,Xid)->c_int,d,output);
        call!(x,"XCopyArea",(*mut Display,Xid,Xid,*mut c_void,c_int,c_int,c_uint,c_uint,c_int,c_int)->c_int,d,pixmap,output,gc,0,0,32,32,0,0);
        sync(x, d)?;
        let image = call!(x,"XGetImage",(*mut Display,Xid,c_int,c_int,c_uint,c_uint,c_ulong,c_int)->*mut c_void,d,output,0,0,64,32,c_ulong::MAX,2);
        if !image.is_null() {
            let get_pixel = sym!(
                x,
                "XGetPixel",
                unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_ulong
            );
            r.composite_pixels = [
                (4, 4, 0x00ff00),
                (16, 16, 0x00ff00),
                (28, 28, 0x00ff00),
                (40, 16, 0x0000ff),
                (60, 28, 0x0000ff),
            ]
            .iter()
            .all(|(px, py, color)| get_pixel(image, *px, *py) & 0xffffff == *color);
            call!(x,"XDestroyImage",(*mut c_void)->c_int,image);
        }
        call!(x,"XDestroyWindow",(*mut Display,Xid)->c_int,d,output);
        call!(x,"XFreeGC",(*mut Display,*mut c_void)->c_int,d,gc);
        call!(x,"XFreePixmap",(*mut Display,Xid)->c_int,d,pixmap);
        call!(&lib,"XCompositeUnredirectWindow",(*mut Display,Xid,c_int)->(),d,window,1);
        call!(x,"XDestroyWindow",(*mut Display,Xid)->c_int,d,window);
        sync(x, d)?;
        if !r.composite_pixels {
            return Err("redirected window pixmap did not contain the painted reference".into());
        }
        Ok(())
    }
}
unsafe fn randr(
    x: &Library,
    lib: &Library,
    d: *mut Display,
    root: Xid,
    r: &mut XCapabilities,
) -> Result<(), String> {
    unsafe {
        let (mut major, mut minor) = (0, 0);
        if call!(&lib,"XRRQueryVersion",(*mut Display,*mut c_int,*mut c_int)->c_int,d,&mut major,&mut minor)
            == 0
        {
            return Err("extension unavailable".into());
        }
        r.randr_version = Some(format!("{major}.{minor}"));
        let (mut event_base, mut error_base) = (0, 0);
        call!(&lib,"XRRQueryExtension",(*mut Display,*mut c_int,*mut c_int)->c_int,d,&mut event_base,&mut error_base);
        call!(&lib,"XRRSelectInput",(*mut Display,Xid,c_int)->(),d,root,1);
        let config = call!(&lib,"XRRGetScreenInfo",(*mut Display,Xid)->*mut c_void,d,root);
        if config.is_null() {
            return Err("screen configuration query unavailable".into());
        }
        let mut rotation = 0u16;
        let current = call!(&lib,"XRRConfigCurrentConfiguration",(*mut c_void,*mut u16)->u16,config,&mut rotation);
        let mut count = 0;
        let sizes = call!(&lib,"XRRConfigSizes",(*mut c_void,*mut c_int)->*mut ScreenSize,config,&mut count);
        let choice = if !sizes.is_null() && (0..=1024).contains(&count) {
            (0..count)
                .filter(|i| {
                    *i != current as i32
                        && (*sizes.add(*i as usize)).width > 0
                        && (*sizes.add(*i as usize)).height > 0
                })
                .min_by_key(|i| {
                    let size = &*sizes.add(*i as usize);
                    (size.width as i64 * size.height as i64
                        - r.dimensions[0] as i64 * r.dimensions[1] as i64)
                        .abs()
                })
        } else {
            None
        };
        let result = if let Some(choice) = choice {
            let size = &*sizes.add(choice as usize);
            let requested = [size.width as u32, size.height as u32];
            // Resolve event functions before changing state, so a missing symbol
            // cannot bypass restoration after a successful mode request.
            let pending = sym!(x, "XPending", unsafe extern "C" fn(*mut Display) -> c_int);
            let next = sym!(
                x,
                "XNextEvent",
                unsafe extern "C" fn(*mut Display, *mut c_void) -> c_int
            );
            r.initial_mode_restored = false;
            let status = call!(&lib,"XRRSetScreenConfig",(*mut Display,*mut c_void,Xid,c_int,u16,c_ulong)->c_int,d,config,root,choice,rotation,0);
            let synced = sync(x, d);
            let actual = geometry(x, d, root);
            let mut observed = false;
            while pending(d) > 0 {
                let mut event = [0 as c_ulong; 24];
                next(d, event.as_mut_ptr().cast());
                if *(event.as_ptr().cast::<c_int>()) == event_base {
                    observed = true;
                }
            }
            // Always restore the provider's initial mode, including a partially successful request.
            call!(&lib,"XRRSetScreenConfig",(*mut Display,*mut c_void,Xid,c_int,u16,c_ulong)->c_int,d,config,root,current as i32,rotation,0);
            let restored = sync(x, d).and_then(|_| geometry(x, d, root));
            if restored? != r.dimensions {
                return Err("provider failed to restore its initial resolution".into());
            }
            r.initial_mode_restored = true;
            if status == 0
                && synced.is_ok()
                && actual.as_ref().is_ok_and(|size| *size == requested)
                && observed
            {
                r.randr_change = Some(requested);
                Ok(())
            } else {
                Err(format!(
                    "provider does not support requested mode change: status={status}, requested={requested:?}, actual={actual:?}, screen-change event={observed}"
                ))
            }
        } else {
            Err(
                "provider advertises no alternate screen sizes; resolution changes unavailable"
                    .into(),
            )
        };
        call!(&lib,"XRRFreeScreenConfigInfo",(*mut c_void)->(),config);
        result
    }
}
