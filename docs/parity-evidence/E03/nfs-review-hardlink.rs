#[cfg(all(test, unix))]
mod independent_review {
 use super::*;
 #[tokio::test]
 async fn hardlink_outside_name_is_mutated_through_export() {
  let root = tempfile::tempdir().unwrap();
  let outside = tempfile::tempdir().unwrap();
  let secret = outside.path().join("secret");
  std::fs::write(&secret,b"outside original").unwrap();
  std::fs::hard_link(&secret, root.path().join("existing")).unwrap();
  let v = Export::open(root.path(),true).unwrap();
  let c = RequestContext {principal: nfsembed::vfs::Principal::Anonymous, client_addr:"127.0.0.1:12345".parse().unwrap(),export_id:nfsembed::vfs::ExportId(1)};
  let key = v.lookup(&c,v.root(),&NfsName::new(b"existing".to_vec()).unwrap()).await.unwrap().object;
  v.write(&c,key,0,b"CHANGED",WriteStability::FileSync).await.unwrap();
  assert_eq!(&std::fs::read(&secret).unwrap()[..7],b"CHANGED");
  v.setattr(&c,key,SetAttributes{size:Some(0),..Default::default()},None).await.unwrap();
  assert_eq!(std::fs::metadata(&secret).unwrap().len(),0);
  println!("Confirmed: NFS write and SETATTR truncate altered the same inode via its name outside the export");
 }
}
