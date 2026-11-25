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
#[path = "services/vnc/wayland.rs"]
mod wayland;

#[path = "services/vnc/keyboard.rs"]
mod keyboard;

#[path = "runtime/manifest.rs"]
mod manifest;
mod runtime {
    pub(crate) use super::manifest::wayland_command;
}


fn main() {
 let args:Vec<String>=std::env::args().collect(); let mode=&args[1]; let out=std::path::PathBuf::from(&args[2]);
 let cancelpath=out.join("cancel"); let cancel=std::sync::Arc::new(move || cancelpath.exists());
 let started=std::time::Instant::now();
 let mut source=match wayland::Wayland::open(mode!="viewonly",cancel) { Ok(s)=>s,Err(e)=>{std::fs::write(out.join("result.json"),serde_json::json!({"open_error":e,"elapsed_ms":started.elapsed().as_millis()}).to_string()).unwrap();return}};
 std::fs::write(out.join("opened"),b"yes").unwrap();
 let mut n=0;
 loop {
  let path=out.join(format!("cmd-{n}.json"));
  if !path.exists(){std::thread::sleep(std::time::Duration::from_millis(10));continue}
  let v:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  let kind=v["kind"].as_str().unwrap();
  if kind=="stop" {drop(source);std::fs::write(out.join(format!("ack-{n}.json")),serde_json::json!({"ok":true,"elapsed_ms":started.elapsed().as_millis()}).to_string()).unwrap();return}
  let result=if kind=="capture" { source.capture().map(|f|{std::fs::write(out.join(format!("frame-{n}.rgba")),&f.rgba).unwrap();serde_json::json!({"width":f.width,"height":f.height})}) }
  else {let event=match kind {"key"=>Input::Key{down:v["down"].as_bool().unwrap(),keysym:v["keysym"].as_u64().unwrap() as u32},"pointer"=>Input::Pointer{buttons:v["buttons"].as_u64().unwrap() as u8,x:v["x"].as_u64().unwrap() as u16,y:v["y"].as_u64().unwrap() as u16},"release"=>Input::Release,_=>panic!("unknown")};source.input(event).map(|_|serde_json::json!({"ok":true}))};
  let ack=match result{Ok(v)=>v,Err(e)=>serde_json::json!({"error":e.to_string()})};std::fs::write(out.join(format!("ack-{n}.json")),ack.to_string()).unwrap();n+=1;
 }
}
