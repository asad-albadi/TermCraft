//! Private independent endpoint, with supported -D key-directory override.
//! No account database, home SSH files, password or login-accounting writes.
use harbor_workspace::{model::Session, ssh::ConnectionConfig};
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub struct DropbearServer {
    pub root: tempfile::TempDir,
    pub port: u16,
    child: Child,
    public_key: String,
}
impl DropbearServer {
    pub fn start() -> Self {
        let tools = PathBuf::from(
            std::env::var_os("TERMCRAFT_DROPBEAR_DIR")
                .expect("stage private Dropbear with prepare-dropbear-interop.sh"),
        );
        // Dropbear validates every parent outside account home. Place fixture below
        // this private checkout, whose ancestors up to account home are not writable by others.
        let parent = std::env::current_dir()
            .unwrap()
            .join("target/interop-endpoints");
        fs::create_dir_all(&parent).unwrap();
        let root = tempfile::Builder::new()
            .prefix("dropbear-")
            .tempdir_in(parent)
            .unwrap();
        let key = root.path().join("host");
        assert!(
            Command::new(tools.join("dropbearkey"))
                .args(["-t", "ed25519", "-f"])
                .arg(&key)
                .output()
                .unwrap()
                .status
                .success()
        );
        let key_output = Command::new(tools.join("dropbearkey"))
            .args(["-y", "-f"])
            .arg(&key)
            .output()
            .unwrap();
        assert!(key_output.status.success());
        let public_key = String::from_utf8(key_output.stdout)
            .unwrap()
            .lines()
            .find(|line| line.starts_with("ssh-ed25519 "))
            .unwrap()
            .to_owned();
        assert!(
            Command::new("ssh-keygen")
                .env_remove("LD_PRELOAD")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(root.path().join("client"))
                .status()
                .unwrap()
                .success()
        );
        fs::copy(
            root.path().join("client.pub"),
            root.path().join("authorized_keys"),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new(tools.join("dropbear"))
            .env_remove("LD_PRELOAD")
            .args(["-F", "-m", "-r"])
            .arg(&key)
            .args(["-p", &format!("127.0.0.1:{port}"), "-P"])
            .arg(root.path().join("pid"))
            .arg("-D")
            .arg(root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.path().join("server.log")).unwrap())
            .spawn()
            .unwrap();
        let mut server = Self {
            root,
            port,
            child,
            public_key,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "Dropbear exited: {}",
                server.log()
            );
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return server;
            }
            assert!(
                Instant::now() < deadline,
                "Dropbear did not listen: {}",
                server.log()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn log(&self) -> String {
        fs::read_to_string(self.root.path().join("server.log")).unwrap()
    }
    pub fn config(&self) -> ConnectionConfig {
        let known = self.root.path().join("known_hosts");
        fs::write(
            &known,
            format!("[127.0.0.1]:{} {}\n", self.port, self.public_key),
        )
        .unwrap();
        ConnectionConfig {
            id: 901,
            agent: None,
            defaults: Default::default(),
            clock: None,
            session: Session {
                host: "127.0.0.1".into(),
                port: self.port,
                user: std::env::var("USER").unwrap(),
                key: self.root.path().join("client").display().to_string(),
                directory: self.root.path().display().to_string(),
                command: "printf 'DROPBEAR_SHELL_OK\\n'; exec /bin/sh".into(),
                ..Default::default()
            },
            app_known_hosts: known,
            user_known_hosts: vec![],
        }
    }
}
impl Drop for DropbearServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(path) = std::env::var_os("TERMCRAFT_INTEROP_LOG_DIR") {
            let path = PathBuf::from(path);
            fs::create_dir_all(&path).unwrap();
            fs::copy(
                self.root.path().join("server.log"),
                path.join(format!(
                    "{}.log",
                    self.root.path().file_name().unwrap().to_string_lossy()
                )),
            )
            .unwrap();
        }
    }
}
