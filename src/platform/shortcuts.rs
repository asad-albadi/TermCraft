//! Launch entries contain stable references and argv, never interpolated endpoints.
use crate::model::SessionId;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub enum ShortcutTarget {
    Session(SessionId),
    Group(String),
}
#[derive(Clone, Debug)]
pub struct ShortcutSpec {
    program: PathBuf,
    target: ShortcutTarget,
    workspace: Option<PathBuf>,
    cwd: Option<PathBuf>,
}
fn checked(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
        Err("Shortcut values must be nonempty, bounded text without control characters.".into())
    } else {
        Ok(())
    }
}
fn path_text(path: &Path) -> Result<&str, String> {
    let text = path
        .to_str()
        .ok_or("Shortcut paths require valid Unicode.")?;
    checked(text)?;
    if !path.is_absolute() {
        return Err("Shortcut paths must be absolute.".into());
    }
    Ok(text)
}
impl ShortcutSpec {
    pub fn for_session(id: SessionId) -> Result<Self, String> {
        checked(&id)?;
        Ok(Self {
            program: std::env::current_exe().map_err(|e| e.to_string())?,
            target: ShortcutTarget::Session(id),
            workspace: None,
            cwd: None,
        })
    }
    pub fn for_group(folder: String) -> Result<Self, String> {
        checked(&folder)?;
        let mut spec = Self::for_session("placeholder".into())?;
        spec.target = ShortcutTarget::Group(folder);
        Ok(spec)
    }
    pub fn with_program(mut self, path: PathBuf) -> Result<Self, String> {
        path_text(&path)?;
        self.program = path;
        Ok(self)
    }
    pub fn with_workspace(mut self, path: PathBuf) -> Result<Self, String> {
        path_text(&path)?;
        self.workspace = Some(path);
        Ok(self)
    }
    pub fn with_cwd(mut self, path: PathBuf) -> Result<Self, String> {
        path_text(&path)?;
        self.cwd = Some(path);
        Ok(self)
    }
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec![];
        if let Some(path) = &self.workspace {
            args.extend(["--data-dir".into(), path.to_string_lossy().into_owned()]);
        }
        match &self.target {
            ShortcutTarget::Session(id) => args.extend(["--session".into(), id.clone()]),
            ShortcutTarget::Group(folder) => args.extend(["--group".into(), folder.clone()]),
        }
        if let Some(path) = &self.cwd {
            args.extend(["--cwd".into(), path.to_string_lossy().into_owned()]);
        }
        args
    }
    pub fn extension() -> &'static str {
        if cfg!(windows) {
            "lnk"
        } else if cfg!(target_os = "macos") {
            "command"
        } else {
            "desktop"
        }
    }
    pub fn desktop_entry(&self, name: &str) -> Result<String, String> {
        checked(name)?;
        let program = path_text(&self.program)?;
        if program.contains('=') {
            return Err("Desktop executable paths cannot contain '='.".into());
        }
        let exec = std::iter::once(program.to_owned())
            .chain(self.arguments())
            .map(|s| desktop_arg(&s))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        Ok(format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName={}\nExec={}\nTerminal=false\nCategories=Network;\n",
            name.replace('\\', "\\\\"),
            exec
        ))
    }
    pub fn write_new(&self, path: &Path, name: &str) -> Result<(), String> {
        checked(name)?;
        path_text(&self.program)?;
        if path.extension().and_then(|s| s.to_str()) != Some(Self::extension()) {
            return Err(format!(
                "Choose a .{} shortcut filename.",
                Self::extension()
            ));
        }
        if path.exists() {
            return Err("Shortcut already exists; choose another filename.".into());
        }
        #[cfg(windows)]
        {
            return self.windows_link(path);
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            let content = format!(
                "#!/bin/sh\nexec {}\n",
                std::iter::once(self.program.to_str().unwrap().to_owned())
                    .chain(self.arguments())
                    .map(|s| format!("'{}'", s.replace('\'', "'\\''")))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            #[cfg(not(target_os = "macos"))]
            let content = self.desktop_entry(name)?;
            let mut staged =
                tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing shortcut directory")?)
                    .map_err(|e| e.to_string())?;
            use std::os::unix::fs::PermissionsExt;
            staged
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            staged
                .write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
            staged.as_file().sync_all().map_err(|e| e.to_string())?;
            staged
                .persist_noclobber(path)
                .map_err(|e| e.error.to_string())?;
            Ok(())
        }
    }
    #[cfg(windows)]
    fn windows_link(&self, path: &Path) -> Result<(), String> {
        // Fixed script; user data travels only through environment values, never script text.
        let parent = path.parent().ok_or("Missing shortcut directory")?;
        let stage = tempfile::tempdir_in(parent).map_err(|e| e.to_string())?;
        let output = stage.path().join("launch.lnk");
        let args = self
            .arguments()
            .iter()
            .map(|s| windows_arg(s))
            .collect::<Vec<_>>()
            .join(" ");
        let result=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; $s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:TERMCRAFT_LINK); $s.TargetPath=$env:TERMCRAFT_TARGET; $s.Arguments=$env:TERMCRAFT_ARGS; $s.Save()"]).env("TERMCRAFT_LINK",&output).env("TERMCRAFT_TARGET",&self.program).env("TERMCRAFT_ARGS",args).output().map_err(|e|e.to_string())?;
        if !result.status.success() {
            return Err("Windows could not create the shortcut.".into());
        }
        // hard_link is atomic and refuses replacement, unlike rename on Windows.
        std::fs::hard_link(output, path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn desktop_arg(s: &str) -> Result<String, String> {
    checked(s)?;
    let mut quoted = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c)
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    Ok(quoted.replace('\\', "\\\\"))
}
/// Windows CRT argv quoting, including trailing backslashes before the closing quote.
pub fn windows_arg(s: &str) -> String {
    let mut result = String::from("\"");
    let mut backslashes = 0;
    for c in s.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        if c == '"' {
            result.push_str(&"\\".repeat(backslashes * 2 + 1));
            result.push('"')
        } else {
            result.push_str(&"\\".repeat(backslashes));
            result.push(c)
        }
        backslashes = 0;
    }
    result.push_str(&"\\".repeat(backslashes * 2));
    result.push('"');
    result
}
