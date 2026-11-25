use eframe::egui;
struct Peer{dir:std::path::PathBuf,text:String,last:String}
impl eframe::App for Peer{fn ui(&mut self,root:&mut egui::Ui,_:&mut eframe::Frame){
 let ctx=root.ctx().clone();
 let events=ctx.input(|i|i.events.clone());if !events.is_empty(){use std::io::Write;let mut f=std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("peer-events")).unwrap();writeln!(f,"{events:?}").unwrap();}
 let cmd=std::fs::read_to_string(self.dir.join("peer-command")).unwrap_or_default();if cmd!=self.last{self.last=cmd.clone();if cmd.trim()=="paste"{ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);let mut clip=arboard::Clipboard::new().unwrap();std::fs::write(self.dir.join("peer-native-read"),format!("{:?}",clip.get_text())).unwrap();}if cmd.trim()=="copy"{ctx.copy_text(std::fs::read_to_string(self.dir.join("payload")).unwrap());}}
 egui::CentralPanel::default().show(root,|ui|{ui.heading("V06 Native Clipboard Peer");let r=ui.add(egui::TextEdit::multiline(&mut self.text));r.request_focus();});
 std::fs::write(self.dir.join("peer-text"),&self.text).unwrap();ctx.request_repaint_after(std::time::Duration::from_millis(100));
}}
fn main()->eframe::Result{let dir=std::env::var_os("V06_REPORT").unwrap().into();eframe::run_native("V06 Clipboard Peer",eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_title("V06 Clipboard Peer").with_inner_size([500.,300.]),..Default::default()},Box::new(move |_|Ok(Box::new(Peer{dir,text:String::new(),last:String::new()}))))}
