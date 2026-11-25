//! Per-user opt-in entries. Existing handlers and default application choices are never replaced.
use super::registry_fs::RegistryValue;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};
fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(
            "Cancelled; completed registry changes may remain. Refresh status before continuing"
                .into(),
        )
    } else {
        Ok(())
    }
}
type Entries = BTreeMap<String, RegistryValue>;
const PREFIX: &str = "Software\\Classes\\";
const OWNER: &str = "TermCraftOwnedV1";
pub trait RegistryStore {
    fn snapshot_cancellable(
        &self,
        key: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<Entries>, String> {
        check_cancel(cancel)?;
        let result = self.snapshot(key)?;
        check_cancel(cancel)?;
        Ok(result)
    }
    fn inherited_exists_cancellable(&self, key: &str, cancel: &AtomicBool) -> Result<bool, String> {
        check_cancel(cancel)?;
        let result = self.inherited_exists(key)?;
        check_cancel(cancel)?;
        Ok(result)
    }
    fn create_exclusive_cancellable(
        &mut self,
        key: &str,
        values: &Entries,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        self.create_exclusive(key, values)
    }
    fn remove_exact_cancellable(
        &mut self,
        key: &str,
        values: &Entries,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        self.remove_exact(key, values)
    }

    fn inherited_exists(&self, _key: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn snapshot(&self, key: &str) -> Result<Option<Entries>, String>;
    fn create_exclusive(&mut self, key: &str, values: &Entries) -> Result<(), String>;
    fn remove_exact(&mut self, key: &str, expected: &Entries) -> Result<(), String>;
}
#[derive(Default)]
pub struct MemoryRegistry {
    trees: BTreeMap<String, Entries>,
    inherited: std::collections::BTreeSet<String>,
}
impl MemoryRegistry {
    pub fn put_inherited(&mut self, key: &str) {
        self.inherited.insert(key.into());
    }
    pub fn put_unrelated(&mut self, key: &str, value: &str) {
        if let Some((root, tree)) = self
            .trees
            .iter_mut()
            .find(|(root, _)| key.starts_with(&format!("{root}\\")))
        {
            tree.insert(
                format!("{}|", &key[root.len() + 1..]),
                RegistryValue::string(value),
            );
        } else {
            self.trees.insert(
                key.into(),
                BTreeMap::from([("|".into(), RegistryValue::string(value))]),
            );
        }
    }
    pub fn value(&self, key: &str) -> Option<RegistryValue> {
        if let Some(tree) = self.trees.get(key) {
            return tree.get("|").cloned();
        }
        self.trees.iter().find_map(|(root, tree)| {
            key.strip_prefix(&format!("{root}\\"))
                .and_then(|tail| tree.get(&format!("{tail}|")))
                .cloned()
        })
    }
}
impl RegistryStore for MemoryRegistry {
    fn inherited_exists(&self, key: &str) -> Result<bool, String> {
        Ok(self.inherited.contains(key))
    }
    fn snapshot(&self, key: &str) -> Result<Option<Entries>, String> {
        Ok(self.trees.get(key).cloned())
    }
    fn create_exclusive(&mut self, key: &str, values: &Entries) -> Result<(), String> {
        if self.trees.contains_key(key) {
            return Err("Existing registration preserved".into());
        }
        self.trees.insert(key.into(), values.clone());
        Ok(())
    }
    fn remove_exact(&mut self, key: &str, expected: &Entries) -> Result<(), String> {
        if self.trees.get(key) != Some(expected) {
            return Err("Registration changed; preserved".into());
        }
        self.trees.remove(key);
        Ok(())
    }
}
pub struct WindowsIntegration {
    entries: Vec<(String, Entries)>,
    executable: String,
}
fn command(exe: &str, argument: &str, placeholder: &str) -> String {
    let placeholder = if argument == "--windows-activation-cwd" {
        format!("{placeholder}\\.")
    } else {
        placeholder.into()
    };
    format!("\"{exe}\" {argument} \"{placeholder}\"")
}
impl WindowsIntegration {
    pub fn new(executable: &str) -> Result<Self, String> {
        let path = super::path_map::PathMap::to_native(executable)?;
        let executable = path.to_str().ok_or("Executable requires Unicode")?;
        if executable.contains('%') || !executable.to_ascii_lowercase().ends_with(".exe") {
            return Err("Choose the installed TermCraft executable".into());
        }
        let mut entries = Vec::new();
        for (key, label, flag, placeholder, protocol) in [
            (
                "Directory\\shell\\TermCraft.OpenHere",
                "Open TermCraft here",
                "--windows-activation-cwd",
                "%1",
                false,
            ),
            (
                "Directory\\Background\\shell\\TermCraft.OpenHere",
                "Open TermCraft here",
                "--windows-activation-cwd",
                "%V",
                false,
            ),
            (
                "termcraft",
                "URL:TermCraft bookmark",
                "--windows-activation-url",
                "%1",
                true,
            ),
            (
                "TermCraft.Bookmark",
                "TermCraft bookmark",
                "--windows-activation-file",
                "%1",
                false,
            ),
        ] {
            let mut values = BTreeMap::from([
                ("|".into(), RegistryValue::string(label)),
                (format!("|{OWNER}"), RegistryValue::string(executable)),
                (
                    "shell\\open\\command|".into(),
                    RegistryValue::string(&command(executable, flag, placeholder)),
                ),
            ]);
            if key.contains("shell\\TermCraft.OpenHere") {
                let value = values.remove("shell\\open\\command|").unwrap();
                values.insert("command|".into(), value);
            }
            if protocol {
                values.insert("|URL Protocol".into(), RegistryValue::string(""));
            }
            entries.push((format!("{PREFIX}{key}"), values));
        }
        entries.push((
            format!("{PREFIX}.termcraft-link"),
            BTreeMap::from([
                ("|".into(), RegistryValue::string("TermCraft.Bookmark")),
                (format!("|{OWNER}"), RegistryValue::string(executable)),
            ]),
        ));
        Ok(Self {
            entries,
            executable: executable.into(),
        })
    }
    pub fn status_in(&self, store: &impl RegistryStore) -> Result<bool, String> {
        self.status_in_cancellable(store, &AtomicBool::new(false))
    }
    pub fn status_in_cancellable(
        &self,
        store: &impl RegistryStore,
        cancel: &AtomicBool,
    ) -> Result<bool, String> {
        let mut complete = true;
        for (key, values) in &self.entries {
            check_cancel(cancel)?;
            match store.snapshot_cancellable(key, cancel)? {
                Some(found) if found == *values => {}
                None => {
                    if store.inherited_exists_cancellable(key, cancel)? {
                        return Err("Existing machine registration preserved".into());
                    }
                    complete = false;
                }
                _ => {
                    return Err(
                        "Existing or changed registration preserved; automatic replacement refused"
                            .into(),
                    );
                }
            }
        }
        check_cancel(cancel)?;
        Ok(complete)
    }
    pub fn install_into(&self, store: &mut impl RegistryStore) -> Result<(), String> {
        self.install_into_cancellable(store, &AtomicBool::new(false))
    }
    pub fn install_into_cancellable(
        &self,
        store: &mut impl RegistryStore,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let mut missing = Vec::new();
        for (key, values) in &self.entries {
            check_cancel(cancel)?;
            match store.snapshot_cancellable(key, cancel)? {
                None => {
                    if store.inherited_exists_cancellable(key, cancel)? {
                        return Err(
                            "Existing machine handler preserved; installation refused".into()
                        );
                    }
                    missing.push((key, values));
                }
                Some(found) if found == *values => {}
                _ => return Err("Existing handler preserved; installation refused".into()),
            }
        }
        let mut created: Vec<(&String, &Entries)> = Vec::new();
        for (key, values) in missing {
            check_cancel(cancel)?;
            if let Err(error) = store.create_exclusive_cancellable(key, values, cancel) {
                // A cancellation stops all future mutations, including cleanup. Completed owned
                // entries remain visible; partial entries are preserved and require inspection.
                if !cancel.load(Ordering::Relaxed) {
                    for (key, values) in created.into_iter().rev() {
                        let _ = store.remove_exact_cancellable(key, values, cancel);
                    }
                }
                return Err(error);
            }
            created.push((key, values));
        }
        check_cancel(cancel)?;
        Ok(())
    }
    pub fn remove_from(&self, store: &mut impl RegistryStore) -> Result<(), String> {
        self.remove_from_cancellable(store, &AtomicBool::new(false))
    }
    pub fn remove_from_cancellable(
        &self,
        store: &mut impl RegistryStore,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let mut found = Vec::new();
        for (key, values) in &self.entries {
            check_cancel(cancel)?;
            match store.snapshot_cancellable(key, cancel)? {
                None => {}
                Some(actual) if actual == *values => found.push((key, values)),
                _ => return Err("Changed registration preserved; removal refused".into()),
            }
        }
        for (key, values) in found.into_iter().rev() {
            check_cancel(cancel)?;
            store.remove_exact_cancellable(key, values, cancel)?;
        }
        check_cancel(cancel)?;
        Ok(())
    }
    /// Read-only package lifetime guard. Unrelated handlers can remain; references to this
    /// exact installed executable (including edited owned handlers) prevent its deletion.
    pub fn uninstall_guard_in(
        &self,
        store: &impl RegistryStore,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let executable = self.executable.to_lowercase();
        for (key, _) in &self.entries {
            check_cancel(cancel)?;
            if let Some(actual) = store.snapshot_cancellable(key, cancel)? {
                if actual.values().any(|value| {
                    matches!(value.kind, 1 | 2 | 7)
                        && value.display().to_lowercase().contains(&executable)
                }) {
                    return Err("This executable still has Explorer/protocol/file registrations. Use Tools > Windows Explorer integration > Remove mine before uninstalling. Edited registrations are preserved and require inspection.".into());
                }
            }
        }
        check_cancel(cancel)?;
        Ok(())
    }
    #[cfg(windows)]
    pub fn current() -> Result<Self, String> {
        Self::new(
            std::env::current_exe()
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Executable path requires Unicode")?,
        )
    }
    #[cfg(windows)]
    pub fn install_user(&self, cancel: &AtomicBool) -> Result<(), String> {
        self.install_into_cancellable(&mut NativeRegistry, cancel)
    }
    #[cfg(windows)]
    pub fn remove_user(&self, cancel: &AtomicBool) -> Result<(), String> {
        self.remove_from_cancellable(&mut NativeRegistry, cancel)
    }
    #[cfg(windows)]
    pub fn status(&self, cancel: &AtomicBool) -> Result<bool, String> {
        self.status_in_cancellable(&NativeRegistry, cancel)
    }
}
#[cfg(windows)]
pub struct NativeRegistry;
#[cfg(windows)]
impl RegistryStore for NativeRegistry {
    fn inherited_exists(&self, key: &str) -> Result<bool, String> {
        use super::registry_fs::native;
        use windows_sys::Win32::System::Registry::*;
        let Some(relative) = key.strip_prefix(PREFIX) else {
            return Ok(false);
        };
        let mut handle = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CLASSES_ROOT,
                native::wide(relative).as_ptr(),
                0,
                KEY_READ,
                &mut handle,
            )
        };
        if code == 2 {
            return Ok(false);
        }
        if code != 0 {
            return Err(native::error(code));
        }
        drop(native::Key(handle));
        Ok(true)
    }

    fn snapshot(&self, key: &str) -> Result<Option<Entries>, String> {
        self.snapshot_cancellable(key, &AtomicBool::new(false))
    }
    fn snapshot_cancellable(
        &self,
        key: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<Entries>, String> {
        check_cancel(cancel)?;
        use super::registry_fs::{Hive, RegistryFs, RegistryPath, native};
        use windows_sys::Win32::System::Registry::*;
        let mut handle = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                native::wide(key).as_ptr(),
                0,
                KEY_READ,
                &mut handle,
            )
        };
        if code == 2 {
            return Ok(None);
        }
        if code != 0 {
            return Err(native::error(code));
        }
        drop(native::Key(handle));
        let mut result = Entries::new();
        let mut pending = vec![String::new()];
        while let Some(relative) = pending.pop() {
            if result.len() > 256 || pending.len() > 256 || relative.len() > 2048 {
                return Err("Registration exceeds ownership inspection bounds; preserved".into());
            }
            let full = if relative.is_empty() {
                key.into()
            } else {
                format!("{key}\\{relative}")
            };
            let listing = RegistryFs::enumerate(
                &RegistryPath {
                    hive: Hive::CurrentUser,
                    key: full,
                },
                cancel,
            )?;
            // Empty unknown keys must also make ownership differ.
            if !relative.is_empty() && listing.values.is_empty() && listing.keys.is_empty() {
                result.insert(
                    format!("{relative}|<empty-key>"),
                    RegistryValue {
                        kind: u32::MAX,
                        bytes: vec![],
                    },
                );
            }
            for (name, value) in listing.values {
                result.insert(format!("{relative}|{name}"), value);
            }
            for child in listing.keys {
                pending.push(if relative.is_empty() {
                    child
                } else {
                    format!("{relative}\\{child}")
                });
            }
        }
        Ok(Some(result))
    }
    fn create_exclusive(&mut self, key: &str, values: &Entries) -> Result<(), String> {
        self.create_exclusive_cancellable(key, values, &AtomicBool::new(false))
    }
    fn create_exclusive_cancellable(
        &mut self,
        key: &str,
        values: &Entries,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        use super::registry_fs::native;
        use windows_sys::Win32::System::Registry::*;
        let mut handle = std::ptr::null_mut();
        let mut disposition = 0;
        let code = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                native::wide(key).as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                std::ptr::null(),
                &mut handle,
                &mut disposition,
            )
        };
        if code != 0 {
            return Err(native::error(code));
        }
        let root = native::Key(handle);
        if disposition != REG_CREATED_NEW_KEY {
            return Err("Existing registration preserved".into());
        }
        let result = (|| {
            for (location, value) in values {
                check_cancel(cancel)?;
                let (relative, name) = location.split_once('|').ok_or("Invalid registration")?;
                if relative.is_empty() {
                    native::set(root.0, name, value)?;
                } else {
                    let mut sub = std::ptr::null_mut();
                    let code = unsafe {
                        RegCreateKeyExW(
                            root.0,
                            native::wide(relative).as_ptr(),
                            0,
                            std::ptr::null(),
                            REG_OPTION_NON_VOLATILE,
                            KEY_WRITE,
                            std::ptr::null(),
                            &mut sub,
                            std::ptr::null_mut(),
                        )
                    };
                    if code != 0 {
                        return Err(native::error(code));
                    }
                    let sub = native::Key(sub);
                    check_cancel(cancel)?;
                    native::set(sub.0, name, value)?;
                }
            }
            Ok(())
        })();
        // Keep a partially installed tree on failure: never delete data another process may have added.
        result
    }
    fn remove_exact(&mut self, key: &str, expected: &Entries) -> Result<(), String> {
        self.remove_exact_cancellable(key, expected, &AtomicBool::new(false))
    }
    fn remove_exact_cancellable(
        &mut self,
        key: &str,
        expected: &Entries,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        use super::registry_fs::native;
        use windows_sys::Win32::System::Registry::*;
        if self.snapshot_cancellable(key, cancel)?.as_ref() != Some(expected) {
            return Err("Registration changed; preserved".into());
        }
        let mut keys: Vec<String> = expected
            .keys()
            .filter_map(|s| s.split_once('|').map(|(k, _)| k.to_owned()))
            .flat_map(|s| {
                let mut ancestors = vec![s.clone()];
                let mut tail = s.as_str();
                while let Some((parent, _)) = tail.rsplit_once('\\') {
                    ancestors.push(parent.into());
                    tail = parent;
                }
                ancestors
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys.sort_by_key(|s| std::cmp::Reverse(s.len()));
        for relative in keys {
            check_cancel(cancel)?;
            let full = if relative.is_empty() {
                key.into()
            } else {
                format!("{key}\\{relative}")
            };
            let code = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, native::wide(&full).as_ptr()) };
            if code != 0 {
                return Err(native::error(code));
            }
        }
        Ok(())
    }
}

/// Strict early entry used only by the supplied package uninstaller. Runs before workspace IPC.
#[cfg(windows)]
pub fn management_entry(args: impl IntoIterator<Item = String>) -> Option<i32> {
    let args: Vec<_> = args.into_iter().collect();
    if !args
        .iter()
        .any(|s| s == "--windows-integration-uninstall-guard")
    {
        return None;
    }
    let result = if args.len() != 1 {
        Err("Uninstall guard accepts no additional arguments".into())
    } else {
        WindowsIntegration::current().and_then(|integration| {
            integration.uninstall_guard_in(&NativeRegistry, &AtomicBool::new(false))
        })
    };
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            3
        }
    })
}
