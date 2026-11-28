//! Owned TLS/NLA fixture, built in a separate dependency graph from SSH.
use std::{
    net::SocketAddr,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
static FIXTURE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
pub fn fixture_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("TERMCRAFT_V03_RDP_FIXTURE") {
        return PathBuf::from(path);
    }
    FIXTURE
        .get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let manifest = root.join("docs/parity-evidence/rdp-prototype/Cargo.toml");
            let output = Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(manifest)
                .env("CARGO_BUILD_JOBS", "2")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Fixture build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            root.join("docs/parity-evidence/rdp-prototype/target/debug")
                .join(if cfg!(windows) {
                    "termcraft-rdp-engine-evaluation.exe"
                } else {
                    "termcraft-rdp-engine-evaluation"
                })
        })
        .clone()
}
pub fn engine_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("TERMCRAFT_V03_RDP_ENGINE") {
        return PathBuf::from(path);
    }
    static ENGINE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let output = Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(root.join("engines/rdp/Cargo.toml"))
                .env("CARGO_BUILD_JOBS", "2")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Engine build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            root.join("engines/rdp/target/debug")
                .join(if cfg!(windows) {
                    "termcraft-rdp-engine.exe"
                } else {
                    "termcraft-rdp-engine"
                })
        })
        .clone()
}
pub struct Server {
    pub root: tempfile::TempDir,
    pub address: SocketAddr,
    child: Child,
}
impl Server {
    pub fn start() -> Self {
        Self::at("127.0.0.1:0".parse().unwrap())
    }
    pub fn with_cursor() -> Self {
        Self::start_mode("127.0.0.1:0".parse().unwrap(), "cursor")
    }
    pub fn at(address: SocketAddr) -> Self {
        Self::start_mode(address, "default")
    }
    pub fn with_resize() -> Self {
        Self::start_mode("127.0.0.1:0".parse().unwrap(), "resize")
    }
    pub fn ignoring_client_resize() -> Self {
        Self::start_mode("127.0.0.1:0".parse().unwrap(), "ignore-layout")
    }
    pub fn with_shrink() -> Self {
        Self::start_mode("127.0.0.1:0".parse().unwrap(), "shrink")
    }
    fn start_mode(address: SocketAddr, mode: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(fixture_binary())
            .arg("--server-fixture")
            .arg(root.path())
            .arg(address.to_string())
            .arg(mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let mut line = String::new();
            let _ = std::io::BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
        let ready = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(line) => line,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("RDP fixture failed to start: {e}")
            }
        };
        let address = ready
            .trim()
            .strip_prefix("READY ")
            .expect("Fixture readiness")
            .parse()
            .unwrap();
        Self {
            root,
            address,
            child,
        }
    }
    pub fn config(&self, id: u64) -> harbor_workspace::desktop::DesktopConfig {
        let mut cfg = harbor_workspace::desktop::DesktopConfig::new(
            id,
            harbor_workspace::model::Session {
                protocol: "RDP".into(),
                host: "localhost".into(),
                port: self.address.port(),
                user: "fixture-user".into(),
                width: 640,
                height: 480,
                ..Default::default()
            },
            engine_binary(),
        );
        cfg.certificate_authority = Some(self.root.path().join("ca.pem"));
        cfg
    }
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn observations(&self) -> String {
        std::fs::read_to_string(self.root.path().join("observations.txt")).unwrap_or_default()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
