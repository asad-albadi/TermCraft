use harbor_workspace::{credentials::{Secret,vault::Vault},model::{Session,Store}};
use std::{env,fs,path::Path};
fn main(){
 let args:Vec<String>=env::args().collect();let root=Path::new(&args[2]);let vault_path=root.join("credentials.vault");let store_path=root.join("workspace.json");
 const MASTER:&str="owned Debian qualification master 2026";const SECRET:&str="SYNTHETIC_DEBIAN_CREDENTIAL_2026";const ID:&str="owned-debian-fixture";
 if args[1]=="create" {fs::create_dir_all(root).unwrap();let mut vault=Vault::create(&vault_path,Secret::new(MASTER)).unwrap();vault.put(ID.into(),Secret::new(SECRET)).unwrap();let mut store=Store::default();store.sessions.push(Session{id:"owned-offline-profile".into(),name:"Owned upgrade fixture".into(),host:"127.0.0.1".into(),credential_ref:Some(ID.into()),..Default::default()});store.preferences.font_size=19.0;store.save(&store_path).unwrap();}
 if args[1]=="corrupt" {assert!(Store::load(&store_path).is_err());assert!(Vault::unlock(&vault_path,Secret::new(MASTER)).is_err());println!("Corrupt store/vault rejected without replacement");return}
 let store=Store::load(&store_path).unwrap();assert_eq!(store.sessions[0].credential_ref.as_deref(),Some(ID));assert_eq!(store.preferences.font_size,19.0);let vault=Vault::unlock(&vault_path,Secret::new(MASTER)).unwrap();assert_eq!(vault.get(&ID.into()).unwrap().expose(),SECRET);assert!(Vault::unlock(&vault_path,Secret::new("wrong owned master")).is_err());
 let bytes=fs::read(&vault_path).unwrap();assert!(!bytes.windows(SECRET.len()).any(|w|w==SECRET.as_bytes()));assert!(!fs::read_to_string(&store_path).unwrap().contains(SECRET));println!("Synthetic credential decrypted; wrong master rejected; workspace reference/preferences retained; secret absent from plaintext");
}
