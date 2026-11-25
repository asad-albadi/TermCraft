use super::ToolProvider;
use std::path::{Path, PathBuf};

pub(super) fn absolute_executable(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(path.to_path_buf())
}

fn system(path: PathBuf, version: &str) -> ToolProvider {
    ToolProvider::System {
        path,
        version: version.into(),
    }
}

pub(super) fn commands(names: &[&str], label: &str) -> Result<ToolProvider, String> {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    for name in names {
        let path = Path::new(name);
        if let Some(path) = absolute_executable(path) {
            return Ok(system(path, "system-managed"));
        }
        if path.components().count() != 1 {
            continue;
        }
        for directory in std::env::split_paths(&paths).filter(|path| path.is_absolute()) {
            let candidate = directory.join(path);
            if let Some(path) = absolute_executable(&candidate) {
                return Ok(system(path, "system-managed"));
            }
            #[cfg(windows)]
            if candidate.extension().is_none() {
                if let Some(path) = absolute_executable(&candidate.with_extension("exe")) {
                    return Ok(system(path, "system-managed"));
                }
            }
        }
    }
    Err(format!(
        "The selected {label} provider is unavailable. Install the required system program."
    ))
}

pub(super) fn chromium() -> Result<ToolProvider, String> {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;

        let path = absolute_executable(Path::new("/usr/lib/chromium/chromium"))
            .ok_or("The qualified system Chromium browser is unavailable")?;
        let mut header = [0; 64];
        std::fs::File::open(&path)
            .and_then(|mut file| file.read_exact(&mut header))
            .map_err(|error| {
                format!("Could not check the installed Chromium executable: {error}")
            })?;
        if !super::manifest::native_header(&header) {
            return Err("The installed Chromium browser has an unsupported architecture".into());
        }
        Ok(system(path, "Arch Linux system-managed Chromium"))
    }
    #[cfg(target_os = "macos")]
    {
        let cpu = match std::env::consts::ARCH {
            "x86_64" => 0x01000007,
            "aarch64" => 0x0100000c,
            _ => return Err("Embedded Browser does not support this macOS architecture".into()),
        };
        let suffixes = [
            "Chromium.app/Contents/MacOS/Chromium",
            "Google Chrome.app/Contents/MacOS/Google Chrome",
        ];
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            let root = PathBuf::from(home);
            if root.is_absolute() {
                roots.push(root.join("Applications"));
            }
        }
        for root in roots {
            for suffix in suffixes {
                let Some(path) = absolute_executable(&root.join(suffix)) else {
                    continue;
                };
                let Ok(mut image) = std::fs::File::open(&path) else {
                    continue;
                };
                if crate::browser::native_macos::verify(&mut image, cpu).is_ok() {
                    return Ok(system(
                        path,
                        "macOS system-managed Chromium browser; native qualification pending",
                    ));
                }
            }
        }
        Err("Install a native Chromium or Google Chrome application in Applications to use Embedded Browser".into())
    }
    #[cfg(windows)]
    {
        crate::browser::native_windows::installed()
            .map(|path| {
                system(
                    path,
                    "Windows system-managed Chromium browser; native qualification pending",
                )
            })
            .ok_or_else(|| {
                "Install a native 64-bit Chromium or Google Chrome to use Embedded Browser".into()
            })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    Err("The qualified system Chromium browser is unavailable".into())
}

pub(super) fn pcap() -> Result<ToolProvider, String> {
    let candidates: &[&str] = if cfg!(target_os = "linux") {
        &[
            "/usr/lib/libpcap.so.1",
            "/usr/lib/x86_64-linux-gnu/libpcap.so.0.8",
            "/usr/lib/aarch64-linux-gnu/libpcap.so.0.8",
            "/usr/lib64/libpcap.so.1",
        ]
    } else if cfg!(target_os = "macos") {
        &["/usr/lib/libpcap.A.dylib"]
    } else {
        &[]
    };
    candidates
        .iter()
        .map(Path::new)
        .find_map(|path| path.canonicalize().ok().filter(|path| path.is_file()))
        .map(|path| system(path, "system-managed libpcap"))
        .ok_or_else(|| "The system libpcap provider is unavailable".into())
}
