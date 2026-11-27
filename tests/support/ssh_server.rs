use harbor_workspace::{model::Session, ssh::ConnectionConfig};
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn sshd_path() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/sbin/sshd"
    } else {
        "/usr/bin/sshd"
    }
}

// A live socket protects selection; a logical claim protects the interval after
// handing the port to ssh/sshd, cancellation, and subsequent rebind checks.
// Claims are conservative across address families because an IPv6 wildcard may
// also occupy IPv4. The mutex covers allocation only, never a running test.
static PORT_CLAIMS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<u16>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

pub struct PortReservation {
    port: u16,
    listener: Option<TcpListener>,
}
impl PortReservation {
    pub fn bind(ip: std::net::IpAddr) -> Self {
        loop {
            let listener = TcpListener::bind((ip, 0)).unwrap();
            if let Ok(reservation) = Self::claim(listener) {
                return reservation;
            }
        }
    }
    pub fn claim(listener: TcpListener) -> std::io::Result<Self> {
        let port = listener.local_addr()?.port();
        let mut claims = PORT_CLAIMS.lock().unwrap_or_else(|p| p.into_inner());
        if !claims.insert(port) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "port is reserved by another owned fixture",
            ));
        }
        Ok(Self {
            port,
            listener: Some(listener),
        })
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn release_listener(&mut self) {
        self.listener.take();
    }
    pub fn take_listener(&mut self) -> TcpListener {
        self.listener.take().unwrap()
    }
}
impl Drop for PortReservation {
    fn drop(&mut self) {
        self.listener.take();
        PORT_CLAIMS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.port);
    }
}

pub struct SshServer {
    pub root: PathBuf,
    pub port: u16,
    child: Child,
    _port: PortReservation,
}
impl SshServer {
    pub fn start(sftp: bool) -> Self {
        Self::start_with_sftp_options(sftp, "")
    }
    pub fn start_with_sftp_options(sftp: bool, options: &str) -> Self {
        Self::start_with_settings(sftp, options, "")
    }
    pub fn start_with_settings(sftp: bool, options: &str, extra: &str) -> Self {
        // Reserve the selection→bind interval across parallel fixture startups.
        static STARTUP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _startup = STARTUP_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Keep SFTP paths canonical and leave room for nested AF_UNIX fixture
        // sockets under Darwin's short sockaddr_un limit.
        let temporary = if cfg!(target_os = "macos") {
            PathBuf::from("/private/tmp")
        } else {
            std::env::temp_dir().canonicalize().unwrap()
        };
        let root = tempfile::Builder::new()
            .prefix("termcraft-ssh-")
            .tempdir_in(temporary)
            .unwrap()
            .keep();
        for key in ["host", "client"] {
            let result = Command::new("ssh-keygen")
                .env_remove("LD_PRELOAD")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(root.join(key))
                .status()
                .unwrap();
            assert!(result.success());
        }
        fs::copy(root.join("client.pub"), root.join("authorized_keys")).unwrap();
        let mut reservation = PortReservation::bind("127.0.0.1".parse().unwrap());
        let port = reservation.port();
        reservation.release_listener();
        let config = format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}/host\nPidFile {}/pid\nAuthorizedKeysFile {}/authorized_keys\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nPermitRootLogin yes\nLogLevel VERBOSE\n{}\n",
            root.display(),
            root.display(),
            root.display(),
            if sftp {
                format!("Subsystem sftp internal-sftp {options}")
            } else {
                String::new()
            }
        );
        fs::write(root.join("sshd_config"), format!("{config}\n{extra}\n")).unwrap();
        let mut child = Command::new(sshd_path())
            .env_remove("LD_PRELOAD")
            .args(["-D", "-e", "-f"])
            .arg(root.join("sshd_config"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.join("server.log")).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "sshd exited: {}",
                fs::read_to_string(root.join("server.log")).unwrap()
            );
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline && child.try_wait().unwrap().is_none(),
                "sshd fixture failed: {}",
                fs::read_to_string(root.join("server.log")).unwrap()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        Self {
            root,
            port,
            child,
            _port: reservation,
        }
    }
    pub fn config(&self, id: u64, trust: bool) -> ConnectionConfig {
        let known = self.root.join("known_hosts");
        if trust {
            let pubkey = fs::read_to_string(self.root.join("host.pub")).unwrap();
            fs::write(&known, format!("[127.0.0.1]:{} {pubkey}", self.port)).unwrap();
        }
        ConnectionConfig {
            id,
            agent: None,
            defaults: Default::default(),
            clock: None,
            session: Session {
                host: "127.0.0.1".into(),
                port: self.port,
                user: std::env::var("USER").unwrap(),
                key: self.root.join("client").to_string_lossy().into(),
                directory: self.root.to_string_lossy().into(),
                command: "exec /bin/sh".into(),
                ..Default::default()
            },
            app_known_hosts: known,
            user_known_hosts: vec![],
        }
    }
    #[cfg(target_os = "linux")]
    pub fn terminate_owned_sftp(&self) {
        let mut queue = vec![self.child.id()];
        while let Some(pid) = queue.pop() {
            if let Ok(children) = fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
                queue.extend(
                    children
                        .split_whitespace()
                        .filter_map(|s| s.parse::<u32>().ok()),
                );
            }
            if let Ok(command) = fs::read(format!("/proc/{pid}/cmdline")) {
                if String::from_utf8_lossy(&command).contains("internal-sftp") {
                    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
                    return;
                }
            }
        }
        panic!("No owned fixture SFTP child found");
    }
    pub fn restart_with_new_host_key(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        for name in ["host", "host.pub"] {
            fs::remove_file(self.root.join(name)).unwrap();
        }
        assert!(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(self.root.join("host"))
                .status()
                .unwrap()
                .success()
        );
        self.child = Command::new(sshd_path())
            .env_remove("LD_PRELOAD")
            .args(["-D", "-e", "-f"])
            .arg(self.root.join("sshd_config"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(self.root.join("server.log")).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(("127.0.0.1", self.port)).is_err() {
            assert!(Instant::now() < deadline && self.child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn accepted_authentications(&self) -> usize {
        fs::read_to_string(self.root.join("server.log"))
            .unwrap()
            .matches("Accepted publickey")
            .count()
    }
}
impl Drop for SshServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Opt-in evidence retention never includes private keys, credentials or config.
        if let Some(directory) = std::env::var_os("TERMCRAFT_INTEROP_LOG_DIR") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).expect("create fixture evidence directory");
            let name = self.root.file_name().unwrap().to_string_lossy();
            fs::copy(
                self.root.join("server.log"),
                directory.join(format!("{name}.log")),
            )
            .expect("retain owned OpenSSH server log");
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
