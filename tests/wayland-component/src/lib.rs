#![allow(dead_code)]
use std::io;
use zeroize::Zeroizing;
const MAX_PIXELS: usize = 8_388_608;
#[derive(Clone, Debug)]
struct Frame {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
}
enum Input {
    Key { down: bool, keysym: u32 },
    Pointer { buttons: u8, x: u16, y: u16 },
    Release,
    Clipboard(Zeroizing<String>),
}
trait Source: Send {
    fn capture(&mut self) -> io::Result<Frame>;
    fn input(&mut self, event: Input) -> io::Result<()>;
}
#[path = "../../../src/services/vnc/wayland.rs"]
mod wayland;

#[path = "../../../src/services/vnc/keyboard.rs"]
mod keyboard;

#[path = "../../../src/runtime/manifest.rs"]
mod manifest;
mod runtime {
    pub(crate) use super::manifest::wayland_command;
}
