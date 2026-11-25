//! Foreground Iperf providers. No system daemon configuration or global kills.
use super::supervisor::Context;
use crate::runtime::{RuntimeCatalog, ToolId, ToolProvider};
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IperfVersion {
    Two,
    Three,
}
pub struct IperfServiceConfig {
    pub version: IperfVersion,
    pub bind: SocketAddr,
    pub allow_non_loopback: bool,
    pub runtime_root: PathBuf,
    /// Explicit compatibility opt-in. Packaged providers are preferred and verified.
    pub allow_installed_provider: bool,
    pub interval_seconds: u8,
    /// Iperf2 UDP mode; Iperf3 chooses TCP/UDP per client.
    pub udp: bool,
    pub lifetime: Option<Duration>,
}
impl IperfServiceConfig {
    pub fn local(version: IperfVersion, runtime_root: PathBuf) -> Self {
        Self {
            version,
            bind: (
                [127, 0, 0, 1],
                if version == IperfVersion::Two {
                    5001
                } else {
                    5201
                },
            )
                .into(),
            allow_non_loopback: false,
            runtime_root,
            allow_installed_provider: false,
            interval_seconds: 1,
            udp: false,
            lifetime: None,
        }
    }
}
pub(super) struct Listener {
    child: Child,
    address: SocketAddr,
    version: IperfVersion,
    udp: bool,
    packaged: bool,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Listener {
    pub(super) fn bind(config: IperfServiceConfig) -> Result<Self, String> {
        if config.bind.port() == 0
            || !(1..=60).contains(&config.interval_seconds)
            || config.udp && config.version == IperfVersion::Three
        {
            return Err(
                "Choose a nonzero port and a 1–60 second interval. Iperf3 clients select UDP mode."
                    .into(),
            );
        }
        let catalog = RuntimeCatalog::new(&config.runtime_root);
        let tool = if config.version == IperfVersion::Two {
            ToolId::Iperf
        } else {
            ToolId::Iperf3
        };
        let mut command;
        // Keep the immutable verified image alive until the process is spawned.
        let artifact = match catalog.iperf_artifact(tool) {
            Ok(artifact) => Some(artifact),
            Err(error) => {
                let name = if config.version == IperfVersion::Two {
                    "iperf2"
                } else {
                    "iperf3"
                };
                let path = config.runtime_root.join(format!(
                    "runtime/{}-{}/{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    name
                ));
                if std::fs::symlink_metadata(path).is_ok() {
                    return Err(format!(
                        "Packaged Iperf engine failed verification: {error}"
                    ));
                }
                None
            }
        };
        let _sealed: Option<Box<dyn std::any::Any>>;
        if let Some(artifact) = artifact.as_ref() {
            #[cfg(target_os = "linux")]
            {
                let (cmd, sealed) = artifact.sealed_command()?;
                command = cmd;
                _sealed = Some(Box::new(sealed));
            }
            #[cfg(target_os = "macos")]
            {
                let (cmd, sealed) = artifact.snapshot_command()?;
                command = cmd;
                _sealed = Some(Box::new(sealed));
            }
            #[cfg(windows)]
            {
                command = Command::new(&artifact.path);
                _sealed = None;
            }
        } else if config.allow_installed_provider {
            let provider = catalog.resolve_installed_iperf(tool)?;
            let ToolProvider::System { path, .. } = provider else {
                return Err("Installed Iperf provider required.".into());
            };
            validate_installed_version(&path, config.version)?;
            command = Command::new(path);
            _sealed = None;
        } else {
            return Err("The verified packaged Iperf engine is unavailable. Install its runtime package or explicitly select an installed compatibility provider.".into());
        }
        // Reserve/check the requested socket before spawning. Foreground engines own
        // their selected listener; any exit before readiness is a failed startup.
        if config.udp {
            drop(
                std::net::UdpSocket::bind(config.bind)
                    .map_err(|e| format!("Could not bind Iperf: {e}"))?,
            );
        } else {
            drop(TcpListener::bind(config.bind).map_err(|e| format!("Could not bind Iperf: {e}"))?);
        }
        command.args([
            "-s",
            "-B",
            &config.bind.ip().to_string(),
            "-p",
            &config.bind.port().to_string(),
            "-i",
            &config.interval_seconds.to_string(),
        ]);
        if config.udp {
            command.arg("-u");
        }
        if config.bind.is_ipv6() {
            command.arg(if config.version == IperfVersion::Two {
                "-V"
            } else {
                "-6"
            });
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not launch the selected Iperf provider.")?;
        let mut listener = Self {
            child,
            address: config.bind,
            version: config.version,
            udp: config.udp,
            packaged: artifact.is_some(),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if listener
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err("The selected Iperf engine exited before it became ready.".into());
            }
            let ready = if listener.udp {
                std::net::UdpSocket::bind(listener.address).is_err()
            } else {
                TcpStream::connect_timeout(
                    &probe_address(listener.address),
                    Duration::from_millis(50),
                )
                .is_ok()
            };
            if ready {
                thread::sleep(Duration::from_millis(75));
                if listener
                    .child
                    .try_wait()
                    .map_err(|e| e.to_string())?
                    .is_none()
                {
                    return Ok(listener);
                }
            }
            if Instant::now() >= deadline {
                return Err("The Iperf engine did not open its configured listener.".into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    pub(super) fn address(&self) -> io::Result<SocketAddr> {
        Ok(self.address)
    }
    pub(super) fn serve(mut self, context: &Arc<Context>) -> io::Result<()> {
        context.log(if self.packaged {
            "Provider: verified packaged Iperf engine."
        } else {
            "Provider: explicitly selected installed compatibility engine."
        });
        context.log(match self.version {
            IperfVersion::Two => "Iperf 2 foreground provider started.",
            IperfVersion::Three => "Iperf 3 foreground provider started.",
        });
        while !context.cancelled() {
            if self.child.try_wait()?.is_some() {
                return Err(io::Error::other("Iperf provider exited unexpectedly"));
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
}
fn probe_address(mut address: SocketAddr) -> SocketAddr {
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    address
}

fn validate_installed_version(path: &std::path::Path, version: IperfVersion) -> Result<(), String> {
    use std::io::Read;
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not inspect the installed Iperf version.")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Installed Iperf version check did not finish.".into());
            }
        }
    };
    let mut output = String::new();
    child
        .stdout
        .take()
        .ok_or("Missing Iperf version result")?
        .take(4096)
        .read_to_string(&mut output)
        .map_err(|_| "Invalid installed Iperf version response")?;
    let expected = match version {
        IperfVersion::Two => "iperf version 2.",
        IperfVersion::Three => "iperf 3.",
    };
    if !status.success()
        || !output
            .lines()
            .next()
            .is_some_and(|line| line.starts_with(expected))
    {
        return Err("The installed Iperf engine does not match the selected major version.".into());
    }
    Ok(())
}
