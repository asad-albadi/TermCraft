//! Independent target-window key interpretation for the isolated native fixture only.
use std::{
    ffi::{c_char, c_void},
    io::Write,
    net::TcpStream,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use x11rb::{connection::Connection, protocol::xproto::KeyPressEvent};
pub(super) fn interpreted_press(display: &str, authority: &Path, event: &KeyPressEvent) -> u32 {
    let output = std::process::Command::new("/usr/bin/xkbcomp")
        .args(["-xkb", display, "-"])
        .env("XAUTHORITY", authority)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let map = std::ffi::CString::new(output.stdout).unwrap();
    unsafe {
        let lib = libloading::Library::new("libxkbcommon.so.0").unwrap();
        let context_new = *lib
            .get::<unsafe extern "C" fn(u32) -> *mut c_void>(b"xkb_context_new\0")
            .unwrap();
        let keymap_new = *lib
            .get::<unsafe extern "C" fn(*mut c_void, *const c_char, u32, u32) -> *mut c_void>(
                b"xkb_keymap_new_from_string\0",
            )
            .unwrap();
        let state_new = *lib
            .get::<unsafe extern "C" fn(*mut c_void) -> *mut c_void>(b"xkb_state_new\0")
            .unwrap();
        let update = *lib
            .get::<unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32, u32, u32) -> u32>(
                b"xkb_state_update_mask\0",
            )
            .unwrap();
        let lookup = *lib
            .get::<unsafe extern "C" fn(*mut c_void, u32) -> u32>(b"xkb_state_key_get_one_sym\0")
            .unwrap();
        let context_unref = *lib
            .get::<unsafe extern "C" fn(*mut c_void)>(b"xkb_context_unref\0")
            .unwrap();
        let keymap_unref = *lib
            .get::<unsafe extern "C" fn(*mut c_void)>(b"xkb_keymap_unref\0")
            .unwrap();
        let state_unref = *lib
            .get::<unsafe extern "C" fn(*mut c_void)>(b"xkb_state_unref\0")
            .unwrap();
        let context = context_new(0);
        assert!(!context.is_null());
        let keymap = keymap_new(context, map.as_ptr(), 1, 0);
        assert!(!keymap.is_null());
        let state = state_new(keymap);
        assert!(!state.is_null());
        let mask = u16::from(event.state);
        update(
            state,
            u32::from(mask & 255),
            0,
            0,
            0,
            0,
            u32::from((mask >> 13) & 3),
        );
        let result = lookup(state, u32::from(event.detail));
        state_unref(state);
        keymap_unref(keymap);
        context_unref(context);
        result
    }
}
pub(super) fn send_key(viewer: &mut TcpStream, sym: u32, down: bool) {
    let mut b = vec![4, u8::from(down), 0, 0];
    b.extend_from_slice(&sym.to_be_bytes());
    viewer.write_all(&b).unwrap();
}
pub(super) fn next_character(
    x: &x11rb::rust_connection::RustConnection,
    display: &str,
    authority: &Path,
) -> (u32, KeyPressEvent) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(x11rb::protocol::Event::KeyPress(event)) = x.poll_for_event().unwrap() {
            let sym = interpreted_press(display, authority, &event);
            if !(0xffe0..=0xffef).contains(&sym) && sym != 0xfe03 {
                return (sym, event);
            }
        }
        assert!(
            Instant::now() < deadline,
            "target window received no interpreted character"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
