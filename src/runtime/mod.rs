//! Explicit provider selection for TermCraft sessions and native tools.
mod browser_bundle;
mod discovery;
#[cfg(test)]
pub(crate) use browser_bundle::VERIFIED_CHUNKS;
mod manifest;
#[cfg(target_os = "linux")]
pub(crate) use manifest::wayland_command;
#[cfg(target_os = "linux")]
mod mosh;
#[cfg(target_os = "linux")]
mod mosh_dependencies;
#[cfg(target_os = "linux")]
pub(crate) use mosh::{MoshLaunch, prepare_mosh_launch};

pub use manifest::{RuntimeArtifact, RuntimeManifest};

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolId {
    Ssh,
    Sftp,
    Scp,
    Rdp,
    Vnc,
    Ftp,
    Telnet,
    Rlogin,
    Rsh,
    Serial,
    Mosh,
    MoshClient,
    SystemMosh,
    Browser,
    SystemChromium,
    BundledChromium,
    XServer,
    Xdmcp,
    OpenSsh,
    OpenSftp,
    ExternalRdp,
    ExternalVnc,
    ExternalBrowser,
    Shell,
    Clipboard,
    DesktopShortcut,
    KeyGenerator,
    ServerKeyGenerator,
    OpenSshServer,
    SshServer,
    Iperf,
    Iperf3,
    Ping,
    Diagnostics,
    IpReference,
    PacketCapture,
    PcapLibrary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolProvider {
    Embedded,
    Bundled { path: PathBuf, version: String },
    System { path: PathBuf, version: String },
}

pub struct RuntimeCatalog {
    bin: PathBuf,
    resources: PathBuf,
}

impl RuntimeCatalog {
    #[cfg(unix)]
    pub(crate) fn bundled_browser_snapshot(
        &self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(PathBuf, tempfile::TempDir), String> {
        let (provider, snapshot) = browser_bundle::prepare(&self.resources, cancelled)?;
        Ok((provider.executable()?, snapshot))
    }
    /// Resolve an explicitly selected command from the active private package.
    pub fn portable_command(
        packages: &crate::packages::PackageManager,
        name: &str,
    ) -> Result<crate::packages::OwnedCommand, String> {
        packages
            .command("busybox", name)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn portable_command_cancellable(
        packages: &crate::packages::PackageManager,
        name: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<crate::packages::OwnedCommand, String> {
        packages
            .command_cancellable("busybox", name, cancel)
            .map_err(|error| error.to_string())
    }

    pub fn service_runtime_root(&self) -> PathBuf {
        self.resources.clone()
    }

    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            bin: root.clone(),
            resources: root,
        }
    }

    pub fn for_current_executable() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let bin = exe
            .parent()
            .ok_or("Missing TermCraft installation directory")?;
        #[cfg(target_os = "macos")]
        if bin.ends_with("Contents/MacOS") {
            return Ok(Self {
                bin: bin.to_path_buf(),
                resources: bin.with_file_name("Resources"),
            });
        }
        Ok(Self::new(bin))
    }

    pub fn resolve(&self, tool: ToolId) -> Result<ToolProvider, String> {
        match tool {
            ToolId::Ssh
            | ToolId::SshServer
            | ToolId::Sftp
            | ToolId::Scp
            | ToolId::Vnc
            | ToolId::Ftp
            | ToolId::Telnet
            | ToolId::Rlogin
            | ToolId::Rsh
            | ToolId::Serial
            | ToolId::KeyGenerator
            | ToolId::Diagnostics
            | ToolId::IpReference
            | ToolId::PacketCapture => Ok(ToolProvider::Embedded),
            ToolId::Rdp => {
                let verified = self.rdp()?;
                Ok(ToolProvider::Bundled {
                    path: verified.path,
                    version: verified.version,
                })
            }
            ToolId::XServer => crate::x11::bundled_runtime(&self.resources)
                .map(|(path, version)| ToolProvider::Bundled { path, version }),
            ToolId::Mosh | ToolId::MoshClient => self.pinned_mosh(tool),
            ToolId::SystemMosh => discovery::commands(&["mosh"], "installed Mosh"),
            ToolId::Browser | ToolId::SystemChromium => discovery::chromium(),
            ToolId::BundledChromium => browser_bundle::resolve(&self.resources),
            ToolId::OpenSsh => discovery::commands(&["ssh"], "OpenSSH compatibility"),
            ToolId::OpenSftp => discovery::commands(&["sftp"], "OpenSSH SFTP compatibility"),
            ToolId::ExternalRdp => discovery::commands(
                if cfg!(windows) {
                    &["mstsc.exe"]
                } else {
                    &["xfreerdp3", "xfreerdp"]
                },
                "external Remote Desktop",
            ),
            ToolId::ExternalVnc => discovery::commands(
                if cfg!(target_os = "macos") {
                    &["open"]
                } else {
                    &["vncviewer"]
                },
                "external VNC viewer",
            ),
            ToolId::ExternalBrowser => discovery::commands(
                if cfg!(windows) {
                    &["explorer.exe"]
                } else if cfg!(target_os = "macos") {
                    &["open"]
                } else {
                    &["xdg-open"]
                },
                "external browser",
            ),
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            ToolId::Xdmcp => crate::x11::bundled_runtime(&self.resources)
                .map(|(path, version)| ToolProvider::Bundled { path, version }),
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            ToolId::Xdmcp => discovery::commands(
                if cfg!(windows) {
                    &["vcxsrv.exe"]
                } else {
                    &["Xephyr"]
                },
                "external XDMCP",
            ),
            ToolId::Shell => {
                discovery::commands(&[&crate::connection::default_shell()], "local shell")
            }
            ToolId::Clipboard => discovery::commands(&["wl-paste"], "Wayland clipboard"),
            ToolId::DesktopShortcut => discovery::commands(&["powershell.exe"], "desktop shortcut"),
            ToolId::PcapLibrary => discovery::pcap(),
            ToolId::ServerKeyGenerator => {
                discovery::commands(&["ssh-keygen"], "server host-key generation")
            }
            ToolId::OpenSshServer => discovery::commands(&["sshd"], "OpenSSH server"),
            ToolId::Iperf | ToolId::Iperf3 => {
                let artifact = self.iperf_artifact(tool)?;
                Ok(ToolProvider::Bundled {
                    path: artifact.path,
                    version: artifact.version,
                })
            }
            ToolId::Ping => discovery::commands(&["ping"], "system ping"),
        }
    }

    pub(crate) fn iperf_artifact(
        &self,
        tool: ToolId,
    ) -> Result<manifest::VerifiedArtifact, String> {
        let name = match tool {
            ToolId::Iperf => "iperf2",
            ToolId::Iperf3 => "iperf3",
            _ => return Err("Invalid Iperf engine selection".into()),
        };
        let manifest: RuntimeManifest =
            serde_json::from_str(include_str!("../../packaging/iperf/runtime-manifest.json"))
                .map_err(|_| "Invalid packaged Iperf manifest")?;
        manifest
            .artifacts
            .iter()
            .find(|a| {
                a.tool == name
                    && a.platform == std::env::consts::OS
                    && a.architecture == std::env::consts::ARCH
            })
            .ok_or("No qualified packaged Iperf engine for this platform")?
            .verify(&self.resources)
    }
    pub fn resolve_installed_iperf(&self, tool: ToolId) -> Result<ToolProvider, String> {
        match tool {
            ToolId::Iperf => discovery::commands(&["iperf"], "installed Iperf 2 compatibility"),
            ToolId::Iperf3 => discovery::commands(&["iperf3"], "installed Iperf 3 compatibility"),
            _ => Err("Invalid installed Iperf selection".into()),
        }
    }
    pub(crate) fn rdp(&self) -> Result<manifest::VerifiedArtifact, String> {
        manifest::PackageManifest::read(&self.resources)?.rdp(&self.bin)
    }

    fn mosh_artifact(&self, tool: ToolId) -> Result<manifest::VerifiedArtifact, String> {
        let manifest = RuntimeManifest::pinned();
        let artifact = manifest
            .artifacts
            .iter()
            .find(|item| item.tool == tool.id())
            .ok_or("Missing pinned Mosh runtime declaration")?;
        artifact.verify(&self.resources)
    }

    fn bundled_mosh(
        &self,
    ) -> Result<
        (
            manifest::VerifiedArtifact,
            manifest::VerifiedArtifact,
            PathBuf,
        ),
        String,
    > {
        let wrapper = self.mosh_artifact(ToolId::Mosh)?;
        let client = self.mosh_artifact(ToolId::MoshClient)?;
        #[cfg(target_os = "linux")]
        mosh_dependencies::resolve(&self.resources)?;
        #[cfg(not(target_os = "linux"))]
        return Err("No qualified bundled Mosh dependency closure for this platform".into());
        let ssh = self
            .resources
            .join("runtime/linux-x86_64/mosh-deps/bin/ssh");
        Ok((wrapper, client, ssh))
    }

    pub(crate) fn bundled_mosh_paths(&self) -> Result<(PathBuf, PathBuf, PathBuf), String> {
        let (wrapper, client, ssh) = self.bundled_mosh()?;
        Ok((wrapper.path, client.path, ssh))
    }

    fn pinned_mosh(&self, tool: ToolId) -> Result<ToolProvider, String> {
        let verified = if tool == ToolId::Mosh {
            self.bundled_mosh()?.0
        } else {
            self.mosh_artifact(tool)?
        };
        Ok(ToolProvider::Bundled {
            path: verified.path,
            version: verified.version,
        })
    }
}

impl ToolId {
    fn id(self) -> &'static str {
        match self {
            Self::Mosh => "mosh",
            Self::MoshClient => "mosh-client",
            _ => unreachable!("only static Mosh artifacts use a tool ID"),
        }
    }
}

pub(crate) fn system_program(name: &str) -> Option<PathBuf> {
    discovery::commands(&[name], name).ok()?.executable().ok()
}

impl ToolProvider {
    pub fn executable(self) -> Result<PathBuf, String> {
        match self {
            Self::Bundled { path, .. } | Self::System { path, .. } => Ok(path),
            Self::Embedded => Err("This provider runs inside TermCraft".into()),
        }
    }
}

#[cfg(test)]
mod browser_bundle_contract_tests {
    use super::*;
    #[test]
    fn missing_bundle_never_falls_back_to_host_chromium() {
        let root = tempfile::tempdir().unwrap();
        let error = RuntimeCatalog::new(root.path())
            .resolve(ToolId::BundledChromium)
            .unwrap_err();
        assert!(error.contains("bundled Chromium"), "{error}");
    }
}
