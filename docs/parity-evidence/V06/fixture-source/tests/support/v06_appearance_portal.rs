struct Settings{path:std::path::PathBuf}
#[zbus::interface(name="org.freedesktop.portal.Settings")]
impl Settings {
 fn read(&self,namespace:&str,key:&str)->zbus::fdo::Result<zbus::zvariant::OwnedValue>{
 if namespace!="org.freedesktop.appearance"||key!="color-scheme"{return Err(zbus::fdo::Error::InvalidArgs("unexpected key".into()))}
 let value:u32=std::fs::read_to_string(&self.path).unwrap().trim().parse().unwrap();Ok(zbus::zvariant::OwnedValue::from(value))
 }
}
fn main(){let path=std::env::args_os().nth(1).unwrap().into();let _conn=zbus::blocking::connection::Builder::session().unwrap().name("org.freedesktop.portal.Desktop").unwrap().serve_at("/org/freedesktop/portal/desktop",Settings{path}).unwrap().build().unwrap();println!("V06 private portal ready");loop{std::thread::sleep(std::time::Duration::from_secs(1));}}
