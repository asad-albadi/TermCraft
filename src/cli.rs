//! Validated argv shared by startup, IPC and generated shortcuts.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const HELP: &str = "TermCraft remote workspace\n  --data-dir <directory>  Choose an isolated workspace\n  --session <saved-id>    Open a saved session by stable ID\n  --group <folder>        Launch folder and subfolders\n  --new-tab               Open a local terminal\n  --command <text>        Run a command in a new local or --session terminal\n  --macro <name>          Run a named saved macro in a new terminal\n  --cwd <directory>       Set local starting directory\n  --exit-when-done        Close the created terminals when they finish\n  --screenshot <file.png> Capture a separate 120-frame instance and exit\n  --open-url <url>        Open a safe TermCraft bookmark/folder URL\n  --open-file <file>      Open a .termcraft-link bookmark\n  --version               Show the compiled version and exit\n  --help                  Show this help\n";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum CliAction {
    Activate,
    Help,
    Screenshot {
        path: PathBuf,
    },
    NewTab {
        cwd: Option<PathBuf>,
        exit_when_done: bool,
    },
    OpenSession {
        id: String,
        cwd: Option<PathBuf>,
        exit_when_done: bool,
    },
    OpenGroup {
        folder: String,
        cwd: Option<PathBuf>,
        exit_when_done: bool,
    },
    RunCommand {
        command: String,
        session: Option<String>,
        cwd: Option<PathBuf>,
        exit_when_done: bool,
    },
    RunMacro {
        name: String,
        session: Option<String>,
        cwd: Option<PathBuf>,
        exit_when_done: bool,
    },
    OpenFolder {
        path: PathBuf,
        exit_when_done: bool,
    },
}
#[derive(Debug)]
pub struct Invocation {
    pub data_dir: Option<PathBuf>,
    pub action: CliAction,
}
pub struct Cli;
fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.is_empty() || value.starts_with("--") {
        return Err(format!("{flag} requires a value"));
    }
    Ok(value)
}
impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<CliAction, String> {
        Ok(Self::parse_invocation(args)?.action)
    }
    pub fn parse_invocation(args: impl IntoIterator<Item = String>) -> Result<Invocation, String> {
        let args: Vec<String> = args.into_iter().collect();
        if let Some(kind) = args.first().filter(|s| {
            matches!(
                s.as_str(),
                "--windows-activation-url"
                    | "--windows-activation-file"
                    | "--windows-activation-cwd"
            )
        }) {
            // Shell-expanded activation text must never introduce separately parsed options.
            if args.len() != 2 {
                return Err("Windows activation accepts exactly one payload and no workspace/action options".into());
            }
            let action = match kind.as_str() {
                "--windows-activation-url" => crate::platform::launch::open_url(&args[1])?,
                "--windows-activation-file" => {
                    crate::platform::launch::open_file(std::path::Path::new(&args[1]))?
                }
                _ => CliAction::OpenFolder {
                    path: crate::platform::path_map::PathMap::to_native(&args[1])?,
                    exit_when_done: false,
                },
            };
            action.validate()?;
            return Ok(Invocation {
                data_dir: None,
                action,
            });
        }
        let (mut data_dir, mut session, mut group, mut command, mut name, mut cwd, mut screenshot) =
            (None, None, None, None, None, None, None);
        let (mut new_tab, mut exit_when_done, mut help) = (false, false, false);
        let mut args = args.into_iter();
        let mut desktop_launch = None;
        let mut flags = std::collections::HashSet::new();
        let mut total = 0usize;
        while let Some(flag) = args.next() {
            total += flag.len();
            if !flags.insert(flag.clone()) {
                return Err(format!("Duplicate option: {flag}"));
            }
            match flag.as_str() {
                "--open-url" | "--open-file" => {
                    if desktop_launch.is_some() {
                        return Err("Choose one desktop launch action.".into());
                    }
                    desktop_launch = Some((flag.clone(), value(&mut args, &flag)?));
                }
                "--data-dir" => data_dir = Some(PathBuf::from(value(&mut args, &flag)?)),
                "--session" => session = Some(value(&mut args, &flag)?),
                "--group" => group = Some(value(&mut args, &flag)?),
                "--command" => command = Some(value(&mut args, &flag)?),
                "--macro" => name = Some(value(&mut args, &flag)?),
                "--cwd" => {
                    cwd = Some(crate::platform::path_map::PathMap::local(
                        std::path::Path::new(&value(&mut args, &flag)?),
                    )?)
                }
                "--screenshot" => screenshot = Some(PathBuf::from(value(&mut args, &flag)?)),
                "--new-tab" => new_tab = true,
                "--exit-when-done" => exit_when_done = true,
                "--help" | "-h" => help = true,
                _ => return Err(format!("Unknown option: {flag}")),
            }
            if flags.len() > 16 || total > 65536 {
                return Err("Too many command-line options.".into());
            }
        }
        if let Some(path) = &data_dir {
            check_path(path)?;
        }
        if let Some((kind, value)) = desktop_launch {
            if flags
                .iter()
                .any(|f| !matches!(f.as_str(), "--open-url" | "--open-file" | "--data-dir"))
            {
                return Err(
                    "Desktop launch cannot be combined with terminal commands or other actions."
                        .into(),
                );
            }
            let action = if kind == "--open-url" {
                crate::platform::launch::open_url(&value)?
            } else {
                crate::platform::launch::open_file(std::path::Path::new(&value))?
            };
            return Ok(Invocation { data_dir, action });
        }
        if help {
            if flags.len() != 1 {
                return Err("Use --help by itself.".into());
            }
            return Ok(Invocation {
                data_dir,
                action: CliAction::Help,
            });
        }
        if let Some(path) = screenshot {
            if flags
                .iter()
                .any(|f| !matches!(f.as_str(), "--screenshot" | "--data-dir"))
            {
                return Err("Screenshot capture cannot be combined with terminal actions.".into());
            }
            let action = CliAction::Screenshot { path };
            action.validate()?;
            return Ok(Invocation { data_dir, action });
        }
        if group.is_some() && (session.is_some() || command.is_some() || name.is_some() || new_tab)
            || command.is_some() && name.is_some()
            || new_tab && session.is_some()
        {
            return Err("Choose one launch target and at most one command or macro.".into());
        }
        let action = if let Some(command) = command {
            CliAction::RunCommand {
                command,
                session,
                cwd,
                exit_when_done,
            }
        } else if let Some(name) = name {
            CliAction::RunMacro {
                name,
                session,
                cwd,
                exit_when_done,
            }
        } else if let Some(folder) = group {
            CliAction::OpenGroup {
                folder,
                cwd,
                exit_when_done,
            }
        } else if let Some(id) = session {
            CliAction::OpenSession {
                id,
                cwd,
                exit_when_done,
            }
        } else if new_tab {
            CliAction::NewTab {
                cwd,
                exit_when_done,
            }
        } else if let Some(path) = cwd {
            CliAction::OpenFolder {
                path,
                exit_when_done,
            }
        } else if exit_when_done {
            return Err("--exit-when-done requires a terminal action.".into());
        } else {
            CliAction::Activate
        };
        action.validate()?;
        if let Some(path) = &data_dir {
            check_path(path)?
        }
        Ok(Invocation { data_dir, action })
    }
}
fn check_text(value: &str, limit: usize, controls: bool) -> Result<(), String> {
    if value.is_empty()
        || value.len() > limit
        || value.contains('\0')
        || controls && value.chars().any(char::is_control)
    {
        Err("Invalid or oversized command-line value.".into())
    } else {
        Ok(())
    }
}
fn check_path(path: &std::path::Path) -> Result<(), String> {
    check_text(
        path.to_str().ok_or("Paths require valid Unicode.")?,
        8192,
        true,
    )
}
impl CliAction {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Activate | Self::Help => {}
            Self::Screenshot { path } | Self::OpenFolder { path, .. } => check_path(path)?,
            Self::NewTab { cwd, .. } => {
                if let Some(p) = cwd {
                    check_path(p)?
                }
            }
            Self::OpenSession { id, cwd, .. } => {
                check_text(id, 256, true)?;
                if let Some(p) = cwd {
                    check_path(p)?
                }
            }
            Self::OpenGroup { folder, cwd, .. } => {
                check_text(folder, 8192, true)?;
                if let Some(p) = cwd {
                    check_path(p)?
                }
            }
            Self::RunCommand {
                command,
                session,
                cwd,
                ..
            } => {
                check_text(command, 32768, false)?;
                if let Some(id) = session {
                    check_text(id, 256, true)?
                }
                if let Some(p) = cwd {
                    check_path(p)?
                }
            }
            Self::RunMacro {
                name, session, cwd, ..
            } => {
                check_text(name, 1024, true)?;
                if let Some(id) = session {
                    check_text(id, 256, true)?
                }
                if let Some(p) = cwd {
                    check_path(p)?
                }
            }
        }
        Ok(())
    }
    /// Resolve relative client paths before forwarding to an instance with another cwd.
    pub fn absolute_paths(&mut self, base: &std::path::Path) {
        let path = match self {
            Self::Screenshot { path } | Self::OpenFolder { path, .. } => Some(path),
            Self::NewTab { cwd, .. }
            | Self::OpenSession { cwd, .. }
            | Self::OpenGroup { cwd, .. }
            | Self::RunCommand { cwd, .. }
            | Self::RunMacro { cwd, .. } => cwd.as_mut(),
            _ => None,
        };
        if let Some(path) = path {
            if !path.is_absolute() {
                *path = base.join(&*path)
            }
        }
    }
}
