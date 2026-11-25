use harbor_workspace::{packages::{PackageIndex,PackageManager},environment::UnixEnvironment,runtime::{RuntimeCatalog,ToolId,ToolProvider},services::{IperfServiceConfig,IperfVersion,ServiceSupervisor,ServiceState}};
use std::{path::{Path,PathBuf},time::{Duration,Instant},net::TcpListener};
fn main(){
 let root=PathBuf::from(std::env::args_os().nth(1).expect("explicit extracted package root")).canonicalize().unwrap();
 let evidence=PathBuf::from(std::env::args_os().nth(2).unwrap());
 println!("EXTRACTED_ROOT={}",root.display());
 let before_home=std::env::var_os("HOME"); let before_path=std::env::var_os("PATH");
 for release in PackageIndex::pinned().available(){
  let private=evidence.join(format!("unix-{}-{}",release.version,std::process::id()));
  let manager=PackageManager::new(private.clone());
  let archive=root.join("unix-packages").join(&release.file_name);
  println!("INSTALL_ARCHIVE={} pinned_archive_sha256={} pinned_binary_sha256={}",archive.display(),release.sha256,release.binary_sha256);
  manager.install(release.clone(),archive).wait().unwrap();
  let launch=UnixEnvironment::new(manager.clone()).launch_config().unwrap();
  assert!(launch.program.starts_with(&private));
  let output=launch.command(["-c","printf 'HOME=%s\\nPATH=%s\\n' \"$HOME\" \"$PATH\"; test -x \"$SHELL\""]).output().unwrap();
  assert!(output.status.success()); let out=String::from_utf8(output.stdout).unwrap();
  assert!(out.contains(&format!("HOME={}\n",launch.directory.display())));
  assert!(out.contains(&format!("PATH={}\n",launch.program.parent().unwrap().display())));
  println!("{}",out);
  let mut shell=launch.spawn_terminal(80,24).unwrap();
  shell.write(b"test -t 0 && test -t 1 && printf 'real-%s\\n' pty; printf 'alphabeta\\n' > note; cat note | sed s/alpha/portable/; exit\n").unwrap();
  let deadline=Instant::now()+Duration::from_secs(7); let mut bytes=Vec::new();
  while Instant::now()<deadline { if let Ok(chunk)=shell.rx.recv_timeout(Duration::from_millis(100)){bytes.extend(chunk);} shell.poll(); if shell.exited{while let Ok(chunk)=shell.rx.try_recv(){bytes.extend(chunk);}break;} }
  assert!(shell.exited,"Unix shell did not exit");
  let out=String::from_utf8_lossy(&bytes); assert!(out.contains("real-pty"),"{out}"); assert!(out.contains("portablebeta"),"{out}"); assert_eq!(std::fs::read(launch.directory.join("note")).unwrap(),b"alphabeta\n");
  println!("UNIX {} PTY output={} PASS",release.version,out);
  assert_eq!(std::env::var_os("HOME"),before_home); assert_eq!(std::env::var_os("PATH"),before_path);
  drop(shell); drop(launch); manager.remove(release.id.clone()).wait().unwrap(); assert!(manager.list().unwrap().is_empty());
 }
 let supervisor=ServiceSupervisor::default(); let catalog=RuntimeCatalog::new(&root);
 for (version,tool) in [(IperfVersion::Two,ToolId::Iperf),(IperfVersion::Three,ToolId::Iperf3)] {
  let provider=catalog.resolve(tool).unwrap(); assert!(matches!(provider,ToolProvider::Bundled{..})); let path=provider.executable().unwrap(); assert!(path.starts_with(&root));
  println!("IPERF_PROVIDER={}",path.display());
  for udp in [false,true] {
   let reservation=TcpListener::bind(("127.0.0.1",0)).unwrap(); let address=reservation.local_addr().unwrap(); drop(reservation);
   let mut config=IperfServiceConfig::local(version,root.clone()); config.bind=address; config.udp=udp&&version==IperfVersion::Two;
   let id=supervisor.start_iperf(config).unwrap(); assert_eq!(supervisor.status(id).unwrap().address,Some(address));
   let mut command=std::process::Command::new(&path); command.args(["-c","127.0.0.1","-p",&address.port().to_string(),"-t","1","-b","1M"]); if udp{command.arg("-u");}
   let output=command.output().unwrap(); println!("IPERF {version:?} udp={udp} address={address} status={} stdout={} stderr={}",output.status,String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
   supervisor.stop(id).unwrap(); let deadline=Instant::now()+Duration::from_secs(4); while supervisor.status(id).unwrap().state!=ServiceState::Stopped {assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(20));}
   assert!(output.status.success()); assert!(String::from_utf8_lossy(&output.stdout).contains("bits/sec"));
   assert!(supervisor.status(id).unwrap().log.iter().any(|line|line.contains("verified packaged")));
   if udp&&version==IperfVersion::Two{assert!(std::net::UdpSocket::bind(address).is_ok());}else{assert!(TcpListener::bind(address).is_ok());}
   println!("IPERF {version:?} udp={udp} traffic and owned stop PASS");
  }
 }
 println!("ALL EXTRACTED PAYLOAD UNIX/IPERF CHECKS PASS");
}
