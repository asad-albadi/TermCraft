//! Approval is a compiled trust decision. A folder-local JSON file cannot opt in
//! an unreviewed executable or replace the checksum of a private build.
pub const WINDOWS_ARTIFACT: &str = "vcxsrv-21.1.16.1-termcraft-private1-windows-x86_64";
pub const MACOS_ARTIFACT: &str = "xquartz-2.8.6-termcraft-private1-macos-universal";
const WINDOWS_APPROVAL: &str =
    include_str!("../../packaging/x11/native/windows-runtime-approval.sha256");
const WINDOWS_REQUIRED: &[&str] = &[
    "vcxsrv.exe",
    "xkbcomp.exe",
    "fonts/misc/fonts.dir",
    "xkbdata/rules/base",
];
const WINDOWS_UNAPPROVED: &str = "The private Windows X server has no approved native build yet. Stock VcXsrv binds all network interfaces and cannot satisfy owned exclusive loopback transport. Build the pinned private transport source and qualify its executable/dependencies on Windows before packaging; System compatibility remains an explicit alternative.";
pub struct NativePin<'a> {
    pub files: &'a [(&'a str, &'a str)],
    pub executable: &'static str,
    pub version: &'static str,
}
pub fn windows_pin() -> Result<&'static NativePin<'static>, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("The private Windows X server is available only on Windows x64.".into());
    }
    static PIN: std::sync::OnceLock<Result<NativePin<'static>, &'static str>> =
        std::sync::OnceLock::new();
    PIN.get_or_init(|| {
        let files = windows_inventory(WINDOWS_APPROVAL)?;
        Ok(NativePin {
            // The single compiled pin is retained for the process lifetime.
            files: files.leak(),
            executable: "vcxsrv.exe",
            version: "21.1.16.1",
        })
    })
    .as_ref()
    .map_err(|message| (*message).into())
}
pub fn macos_pin() -> Result<&'static NativePin<'static>, String> {
    Err("The private macOS X server has no approved relocated and signed native build yet. Stock XQuartz depends on /opt/X11 and system Mach/startx services; its inherited-listener path bypasses Xauthority. Build the pinned private direct-entry/authenticated-listener source and qualify it on macOS before packaging; System compatibility remains an explicit alternative.".into())
}

fn windows_inventory(approval: &str) -> Result<Vec<(&str, &str)>, &'static str> {
    let mut files = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for line in approval
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let invalid = "Invalid compiled Windows private X file inventory.";
        let (digest, name) = line.split_once("  ").ok_or(invalid)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !name.split('/').all(valid_windows_component)
            || !names.insert(name.to_ascii_lowercase())
        {
            return Err(invalid);
        }
        files.push((name, digest));
    }
    if files.is_empty() {
        return Err(WINDOWS_UNAPPROVED);
    }
    if !WINDOWS_REQUIRED
        .iter()
        .all(|name| files.iter().any(|file| file.0 == *name))
    {
        return Err("Compiled Windows private X file inventory lacks a required runtime file.");
    }
    Ok(files)
}

fn valid_windows_component(name: &str) -> bool {
    if name.is_empty()
        || name.ends_with('.')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_+@(),.-".contains(&b))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        && !stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|number| number.len() == 1 && number.as_bytes()[0].is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_inventory_requires_exact_paths_and_has_no_shipping_pin() {
        assert_eq!(windows_inventory(WINDOWS_APPROVAL), Err(WINDOWS_UNAPPROVED));
        let digest = "a".repeat(64);
        let valid = WINDOWS_REQUIRED
            .iter()
            .map(|name| format!("{digest}  {name}\n"))
            .collect::<String>();
        assert_eq!(
            windows_inventory(&valid).unwrap().len(),
            WINDOWS_REQUIRED.len()
        );
        for invalid in [
            "NUL.txt",
            "read/fonts/AUX",
            "dir/COM1.xkb",
            "dir/file.",
            "../outside",
            "VCXSRV.EXE",
        ] {
            assert!(
                windows_inventory(&format!("{valid}{digest}  {invalid}\n")).is_err(),
                "{invalid}"
            );
        }
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn a_runtime_local_inventory_cannot_override_compiled_windows_approval() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("x11/windows-x86_64-private1");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("vcxsrv.exe"), b"abc").unwrap();
        std::fs::write(
            package.join("windows-runtime-approval.sha256"),
            b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  vcxsrv.exe\n",
        )
        .unwrap();
        assert_eq!(
            super::super::native_windows::bundled_runtime(root.path()).unwrap_err(),
            WINDOWS_UNAPPROVED
        );
    }
}
