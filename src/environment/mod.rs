//! Child-only configuration for the private Portable Unix tools.
use crate::{
    packages::{OwnedCommand, PackageManager},
    runtime::RuntimeCatalog,
};
use cap_fs_ext::DirExt;
use std::{
    ffi::{OsStr, OsString},
    io,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct UnixEnvironment {
    packages: PackageManager,
}

#[derive(Clone, Debug)]
pub struct Launch {
    pub program: PathBuf,
    pub version: String,
    pub directory: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    _command: OwnedCommand,
}

impl UnixEnvironment {
    pub fn new(packages: PackageManager) -> Self {
        Self { packages }
    }

    pub fn launch_config(&self) -> Result<Launch, String> {
        self.command_config("ash")
    }

    pub fn launch_config_cancellable(&self, cancel: &AtomicBool) -> Result<Launch, String> {
        self.command_config_cancellable("ash", cancel)
    }

    pub fn command_config(&self, name: &str) -> Result<Launch, String> {
        self.command_config_cancellable(name, &AtomicBool::new(false))
    }

    fn command_config_cancellable(
        &self,
        name: &str,
        cancel: &AtomicBool,
    ) -> Result<Launch, String> {
        let command = RuntimeCatalog::portable_command_cancellable(&self.packages, name, cancel)?;
        if cancel.load(Ordering::Acquire) {
            return Err("Portable Unix startup cancelled".into());
        }
        let root = self.packages.root();
        let directory = root.join("home");
        let owned = self
            .packages
            .open_root()
            .map_err(|error| error.to_string())?;
        match owned.create_dir("home") {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        owned
            .open_dir_nofollow("home")
            .map_err(|error| error.to_string())?;
        let bin = command
            .path
            .parent()
            .ok_or("Missing portable tools directory")?;
        let path = std::env::join_paths([bin]).map_err(|error| error.to_string())?;
        let mut environment = vec![
            ("PATH".into(), path),
            ("HOME".into(), directory.as_os_str().into()),
            (
                "SHELL".into(),
                bin.join(if cfg!(windows) { "ash.exe" } else { "ash" })
                    .into_os_string(),
            ),
            ("ENV".into(), directory.join(".ashrc").into_os_string()),
            ("TERMCRAFT_UNIX_ROOT".into(), root.as_os_str().into()),
            (
                "TERMCRAFT_UNIX_VERSION".into(),
                command.version.as_str().into(),
            ),
        ];
        if cfg!(windows) {
            environment.push(("BB_SYSTEMROOT".into(), root.as_os_str().into()));
        }
        Ok(Launch {
            program: command.path.clone(),
            version: command.version.clone(),
            directory,
            environment,
            _command: command,
        })
    }
}

impl Launch {
    /// Keep this launch configuration alive until the process tree has exited.
    pub fn command(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(&self.directory)
            .envs(self.environment.iter().map(|(key, value)| (key, value)));
        command
    }

    pub fn spawn_terminal(
        &self,
        columns: u16,
        rows: u16,
    ) -> Result<crate::process::Process, String> {
        self.spawn_terminal_cancellable(columns, rows, &AtomicBool::new(false))
    }

    pub fn spawn_terminal_cancellable(
        &self,
        columns: u16,
        rows: u16,
        cancel: &AtomicBool,
    ) -> Result<crate::process::Process, String> {
        crate::process::Process::spawn_with_environment_cancellable(
            self.program
                .to_str()
                .ok_or("Portable Unix installation path is not valid Unicode")?,
            &["-i".into()],
            Some(&self.directory),
            columns,
            rows,
            &self.environment,
            cancel,
        )
    }
}
