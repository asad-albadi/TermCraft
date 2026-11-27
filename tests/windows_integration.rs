use harbor_workspace::platform::{
    path_map::PathMap,
    windows_integration::{MemoryRegistry, WindowsIntegration},
};
#[test]
fn drive_path_mapping_handles_unicode() {
    assert_eq!(
        PathMap::to_native("/drives/c/資料/a & b")
            .unwrap()
            .to_str()
            .unwrap(),
        "C:\\資料\\a & b"
    );
    assert_eq!(
        PathMap::to_native(r"\\server\share\資料")
            .unwrap()
            .to_str()
            .unwrap(),
        r"\\server\share\資料"
    );
    for bad in [
        "/drives/c/../escape",
        "/drives/c/a\n",
        "/drives/cc/a",
        "C:",
        r"C:\a\..\b",
        r"\\?\C:\a",
        r"C:\a:stream",
        r"C:\CON",
    ] {
        assert!(PathMap::to_native(bad).is_err(), "{bad:?}");
    }
}
#[test]
fn uninstall_removes_only_owned_registry_entries() {
    let mut registry = MemoryRegistry::default();
    let integration = WindowsIntegration::new(r"C:\Program Files\TermCraft\TermCraft.exe").unwrap();
    registry.put_unrelated("Software\\Classes\\unrelated", "keep");
    integration.install_into(&mut registry).unwrap();
    assert!(integration.status_in(&registry).unwrap());
    integration.remove_from(&mut registry).unwrap();
    assert_eq!(
        registry.value("Software\\Classes\\unrelated"),
        Some(harbor_workspace::platform::registry_fs::RegistryValue::string("keep"))
    );
}
#[test]
fn registration_preserves_existing_or_changed_handlers() {
    let integration = WindowsIntegration::new(r"C:\Tools\TermCraft.exe").unwrap();
    let mut registry = MemoryRegistry::default();
    registry.put_unrelated("Software\\Classes\\termcraft", "prior");
    assert!(integration.install_into(&mut registry).is_err());
    assert_eq!(
        registry.value("Software\\Classes\\termcraft"),
        Some(harbor_workspace::platform::registry_fs::RegistryValue::string("prior"))
    );
    let mut registry = MemoryRegistry::default();
    integration.install_into(&mut registry).unwrap();
    registry.put_unrelated(
        "Software\\Classes\\termcraft\\shell\\open\\command",
        "changed",
    );
    assert!(integration.remove_from(&mut registry).is_err());
    assert_eq!(
        registry.value("Software\\Classes\\termcraft\\shell\\open\\command"),
        Some(harbor_workspace::platform::registry_fs::RegistryValue::string("changed"))
    );
}
#[test]
fn typed_registry_value_input_is_bounded_and_literal() {
    use harbor_workspace::platform::registry_fs::RegistryValue;
    assert_eq!(
        RegistryValue::parse_text(4, "4294967295").unwrap().bytes,
        u32::MAX.to_le_bytes()
    );
    assert!(RegistryValue::parse_text(4, "4294967296").is_err());
    assert_eq!(
        RegistryValue::parse_text(3, "00 ff 20").unwrap().bytes,
        vec![0, 255, 32]
    );
    assert!(RegistryValue::parse_text(1, "bad\0value").is_err());
    assert!(RegistryValue::parse_text(99, "anything").is_err());
}

#[cfg(windows)]
#[test]
#[ignore = "temporarily installs the real per-user integration for an explicitly supplied staged executable"]
fn packaged_windows_registration_dispatches_and_guards_uninstall() {
    use harbor_workspace::{
        cli::CliAction,
        ipc::{Binding, InstanceBroker},
    };
    use std::{path::PathBuf, process::Command, sync::atomic::AtomicBool, time::Duration};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            GetExitCodeProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        },
        UI::Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            ShellExecuteExW,
        },
    };

    let executable = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_ACTIVATION_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let expected_image = std::fs::canonicalize(&executable).unwrap();
    let fixture_variable = "TERMCRAFT_TEST_ACTIVATION_LOCALAPPDATA";
    let Some(private_data) = std::env::var_os(fixture_variable) else {
        let private = tempfile::tempdir().unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "packaged_windows_registration_dispatches_and_guards_uninstall",
                "--ignored",
            ])
            .env("LOCALAPPDATA", private.path())
            .env(fixture_variable, private.path())
            .status()
            .unwrap();
        assert!(status.success());
        return;
    };
    assert_eq!(std::env::var_os("LOCALAPPDATA"), Some(private_data.clone()));
    let Binding::Primary(mut workspace) =
        InstanceBroker::bind(&PathBuf::from(private_data).join("Harbor")).unwrap()
    else {
        panic!("private activation workspace was occupied")
    };
    let integration = WindowsIntegration::new(executable.to_str().unwrap()).unwrap();
    let cancel = AtomicBool::new(false);
    assert!(
        !integration.status(&cancel).unwrap(),
        "fixture already registered"
    );
    integration.install_user(&cancel).unwrap();
    struct Registration(WindowsIntegration);
    impl Drop for Registration {
        fn drop(&mut self) {
            let _ = self.0.remove_user(&AtomicBool::new(false));
        }
    }
    let owned = Registration(integration);
    assert!(owned.0.status(&cancel).unwrap());
    let guard = || {
        Command::new(&executable)
            .arg("--windows-integration-uninstall-guard")
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(guard(), Some(3));

    let shell_exit = |url: &str| {
        let target: Vec<u16> = url.encode_utf16().chain([0]).collect();
        let mut launch = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
            lpFile: target.as_ptr(),
            ..Default::default()
        };
        let launched = unsafe { ShellExecuteExW(&mut launch) };
        assert_ne!(launched, 0, "{}", std::io::Error::last_os_error());
        assert!(!launch.hProcess.is_null());
        let mut exit = 0;
        let completed = unsafe {
            let result = WaitForSingleObject(launch.hProcess, 8000) == WAIT_OBJECT_0;
            if !result {
                let mut name = [0u16; 4096];
                let mut length = name.len() as u32;
                if QueryFullProcessImageNameW(launch.hProcess, 0, name.as_mut_ptr(), &mut length)
                    != 0
                    && std::fs::canonicalize(String::from_utf16_lossy(&name[..length as usize]))
                        .ok()
                        .as_ref()
                        == Some(&expected_image)
                {
                    TerminateProcess(launch.hProcess, 1);
                    WaitForSingleObject(launch.hProcess, 2000);
                }
            }
            let read = result && GetExitCodeProcess(launch.hProcess, &mut exit) != 0;
            CloseHandle(launch.hProcess);
            read
        };
        assert!(completed);
        exit
    };
    assert_eq!(shell_exit("termcraft://invalid-qualification-request"), 2);
    assert_eq!(shell_exit("termcraft://session/qualification-fixture"), 0);
    let request = workspace.recv_timeout(Duration::from_secs(4)).unwrap();
    assert!(
        matches!(request.action, CliAction::OpenSession { id, .. } if id == "qualification-fixture")
    );
    owned.0.remove_user(&cancel).unwrap();
    assert_eq!(guard(), Some(0));
}
#[cfg(windows)]
mod native_windows {
    use super::*;
    use harbor_workspace::platform::{
        registry_fs::{ConfirmedWrite, RegistryFs, RegistryPath, RegistryValue},
        windows_integration::{NativeRegistry, RegistryStore},
    };
    use std::{collections::BTreeMap, sync::atomic::AtomicBool};
    use windows_sys::Win32::System::Registry::*;
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    struct Fixture {
        root: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = format!(
                "Software\\TermCraftTestFixtures\\B04-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            NativeRegistry
                .create_exclusive(&root, &BTreeMap::new())
                .unwrap();
            Self { root }
        }
        fn scoped(&self, key: &str) -> String {
            format!(
                "{}\\{}",
                self.root,
                key.strip_prefix("Software\\Classes\\")
                    .expect("fixture only accepts Classes entries")
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.root).as_ptr());
                RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.root).as_ptr());
            }
        }
    }
    impl RegistryStore for Fixture {
        fn snapshot(&self, key: &str) -> Result<Option<BTreeMap<String, RegistryValue>>, String> {
            NativeRegistry.snapshot(&self.scoped(key))
        }
        fn create_exclusive(
            &mut self,
            key: &str,
            values: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            NativeRegistry.create_exclusive(&self.scoped(key), values)
        }
        fn remove_exact(
            &mut self,
            key: &str,
            values: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            NativeRegistry.remove_exact(&self.scoped(key), values)
        }
    }
    #[test]
    fn native_uninstall_removes_only_owned_registry_entries() {
        let mut fixture = Fixture::new();
        let integration =
            WindowsIntegration::new(r"C:\Program Files\TermCraft\TermCraft.exe").unwrap();
        let unrelated = fixture.scoped("Software\\Classes\\unrelated");
        let keep = BTreeMap::from([("|".into(), RegistryValue::string("keep"))]);
        NativeRegistry.create_exclusive(&unrelated, &keep).unwrap();
        integration.install_into(&mut fixture).unwrap();
        assert!(integration.status_in(&fixture).unwrap());
        integration.remove_from(&mut fixture).unwrap();
        assert!(!integration.status_in(&fixture).unwrap());
        assert_eq!(NativeRegistry.snapshot(&unrelated).unwrap(), Some(keep));
    }
    #[test]
    fn native_package_guard_refuses_owned_and_edited_entries_until_cleanup() {
        let mut fixture = Fixture::new();
        let integration = WindowsIntegration::new(r"C:\Tools\TermCraft.exe").unwrap();
        let cancel = AtomicBool::new(false);
        integration.install_into(&mut fixture).unwrap();
        assert!(integration.uninstall_guard_in(&fixture, &cancel).is_err());
        let key = fixture.scoped("Software\\Classes\\termcraft");
        let path = RegistryPath::parse(&format!("HKCU\\{key}\\shell\\open\\command")).unwrap();
        let old = RegistryFs::read(&path, "").unwrap();
        let edited = RegistryValue::string("other command");
        RegistryFs::write_confirmed(
            &ConfirmedWrite {
                path: path.clone(),
                name: "".into(),
                expected: old.clone(),
                replacement: edited.clone(),
            },
            &cancel,
        )
        .unwrap();
        assert!(integration.uninstall_guard_in(&fixture, &cancel).is_err());
        assert!(integration.remove_from(&mut fixture).is_err());
        assert_eq!(RegistryFs::read(&path, "").unwrap(), edited);
        RegistryFs::write_confirmed(
            &ConfirmedWrite {
                path,
                name: "".into(),
                expected: edited,
                replacement: old,
            },
            &cancel,
        )
        .unwrap();
        integration.remove_from(&mut fixture).unwrap();
        assert!(integration.uninstall_guard_in(&fixture, &cancel).is_ok());
    }
    #[test]
    fn native_registry_stale_confirmation_preserves_new_value() {
        let fixture = Fixture::new();
        let key = format!("{}\\values", fixture.root);
        NativeRegistry
            .create_exclusive(
                &key,
                &BTreeMap::from([("|data".into(), RegistryValue::string("old"))]),
            )
            .unwrap();
        let path = RegistryPath::parse(&format!("HKCU\\{key}")).unwrap();
        let old = RegistryFs::read(&path, "data").unwrap();
        let cancel = AtomicBool::new(false);
        let request = ConfirmedWrite {
            path: path.clone(),
            name: "data".into(),
            expected: old,
            replacement: RegistryValue::string("new"),
        };
        RegistryFs::write_confirmed(&request, &cancel).unwrap();
        assert!(RegistryFs::write_confirmed(&request, &cancel).is_err());
        assert_eq!(
            RegistryFs::read(&path, "data").unwrap(),
            RegistryValue::string("new")
        );
    }
    #[test]
    fn native_denied_registry_access_is_reported() {
        use windows_sys::Win32::Security::*;
        let fixture = Fixture::new();
        let key = format!("{}\\denied", fixture.root);
        let mut handle = std::ptr::null_mut();
        let code = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide(&key).as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_ALL_ACCESS,
                std::ptr::null(),
                &mut handle,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, 0);
        let mut acl: ACL = unsafe { std::mem::zeroed() };
        let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
        unsafe {
            assert_ne!(
                InitializeAcl(&mut acl, std::mem::size_of::<ACL>() as u32, ACL_REVISION),
                0
            );
            assert_ne!(
                InitializeSecurityDescriptor(
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                    1
                ),
                0
            );
            assert_ne!(
                SetSecurityDescriptorDacl(
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                    1,
                    &acl,
                    0
                ),
                0
            );
            assert_eq!(
                RegSetKeySecurity(
                    handle,
                    DACL_SECURITY_INFORMATION,
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast()
                ),
                0
            );
        }
        let result = RegistryFs::enumerate(
            &RegistryPath::parse(&format!("HKCU\\{key}")).unwrap(),
            &AtomicBool::new(false),
        );
        // Restore access using the already-open fixture handle before asserting.
        unsafe {
            SetSecurityDescriptorDacl(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                1,
                std::ptr::null_mut(),
                0,
            );
            let restored = RegSetKeySecurity(
                handle,
                DACL_SECURITY_INFORMATION,
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
            );
            RegCloseKey(handle);
            assert_eq!(restored, 0);
        }
        let error = result.unwrap_err();
        assert!(error.contains("Windows error 5"), "{error}");
    }
    #[test]
    fn explorer_launch_preserves_working_directory() {
        use harbor_workspace::cli::{Cli, CliAction};
        let path = r"C:\資料 with spaces\a & b";
        assert_eq!(
            Cli::parse(["--cwd".into(), path.into()]).unwrap(),
            CliAction::OpenFolder {
                path: path.into(),
                exit_when_done: false
            }
        );
        assert_eq!(
            Cli::parse(["--cwd".into(), "/drives/c/資料 with spaces/a & b".into()]).unwrap(),
            CliAction::OpenFolder {
                path: path.into(),
                exit_when_done: false
            }
        );
        assert!(
            harbor_workspace::platform::launch::open_url(
                "termcraft://cwd?path=%2Fdrives%2Fc%2F%E8%B3%87%E6%96%99"
            )
            .is_ok()
        );
    }
}
#[test]
fn shell_registration_rejects_substitution_in_the_executable() {
    assert!(WindowsIntegration::new(r"C:\bad%1\TermCraft.exe").is_err());
}
#[test]
fn explorer_command_handles_drive_roots_without_a_quoted_trailing_backslash() {
    let mut registry = MemoryRegistry::default();
    WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
        .unwrap()
        .install_into(&mut registry)
        .unwrap();
    let value = registry
        .value("Software\\Classes\\Directory\\Background\\shell\\TermCraft.OpenHere\\command")
        .unwrap();
    assert_eq!(
        value.display(),
        r#""C:\Tools\TermCraft.exe" --windows-activation-cwd "%V\.""#
    );
    assert_eq!(
        PathMap::to_native(r"C:\.").unwrap().to_str().unwrap(),
        r"C:\"
    );
}
#[test]
fn cancelled_registry_write_does_not_attempt_platform_access() {
    use harbor_workspace::platform::registry_fs::*;
    use std::sync::atomic::AtomicBool;
    let request = ConfirmedWrite {
        path: RegistryPath::parse("HKCU\\Software\\fixture").unwrap(),
        name: "name".into(),
        expected: RegistryValue::string("before"),
        replacement: RegistryValue::string("after"),
    };
    assert_eq!(
        RegistryFs::write_confirmed(&request, &AtomicBool::new(true)).unwrap_err(),
        "Cancelled"
    );
}
#[test]
fn local_launch_preserves_spaces_unicode_and_metacharacters() {
    use harbor_workspace::cli::{Cli, CliAction};
    let path = std::env::temp_dir().join("資料 and space & $(literal)");
    let action = Cli::parse(["--cwd".into(), path.to_str().unwrap().into()]).unwrap();
    assert_eq!(
        action,
        CliAction::OpenFolder {
            path,
            exit_when_done: false
        }
    );
}

#[test]
fn binary_editor_does_not_truncate_existing_values() {
    let value = harbor_workspace::platform::registry_fs::RegistryValue {
        kind: 3,
        bytes: (0..1024).map(|n| (n % 256) as u8).collect(),
    };
    assert_eq!(
        harbor_workspace::platform::registry_fs::RegistryValue::parse_text(3, &value.display())
            .unwrap(),
        value
    );
}

#[test]
fn multistring_editor_preserves_literal_separators_and_newlines() {
    use harbor_workspace::platform::registry_fs::RegistryValue;
    let original =
        RegistryValue::parse_text(7, r#"["literal | separator","embedded\nnewline","資料"]"#)
            .unwrap();
    assert_eq!(
        RegistryValue::parse_text(7, &original.edit_text().unwrap()).unwrap(),
        original
    );
}

#[test]
fn status_reports_conflicts_even_when_other_entries_are_missing() {
    let integration = WindowsIntegration::new(r"C:\Tools\TermCraft.exe").unwrap();
    let mut registry = MemoryRegistry::default();
    registry.put_unrelated("Software\\Classes\\termcraft", "previous handler");
    assert!(integration.status_in(&registry).is_err());
}

#[test]
fn install_does_not_shadow_an_existing_machine_handler() {
    let integration = WindowsIntegration::new(r"C:\Tools\TermCraft.exe").unwrap();
    let mut registry = MemoryRegistry::default();
    registry.put_inherited("Software\\Classes\\termcraft");
    assert!(integration.install_into(&mut registry).is_err());
    assert!(
        registry
            .value("Software\\Classes\\Directory\\shell\\TermCraft.OpenHere")
            .is_none()
    );
}

#[test]
fn windows_activation_rejects_extra_workspace_options_but_trusted_cli_keeps_them() {
    use harbor_workspace::cli::Cli;
    assert!(
        Cli::parse_invocation([
            "--windows-activation-url".into(),
            "termcraft://session/id".into(),
            "--data-dir".into(),
            r"C:\attacker".into()
        ])
        .is_err()
    );
    assert!(
        Cli::parse_invocation([
            "--windows-activation-url".into(),
            "termcraft://session/id".into()
        ])
        .is_ok()
    );
    let trusted = Cli::parse_invocation([
        "--open-url".into(),
        "termcraft://session/id".into(),
        "--data-dir".into(),
        r"C:\chosen".into(),
    ])
    .unwrap();
    assert_eq!(trusted.data_dir.unwrap().to_str().unwrap(), r"C:\chosen");
}
#[test]
fn uninstall_guard_refuses_owned_or_edited_references_and_preserves_unrelated_entries() {
    use std::sync::atomic::AtomicBool;
    let integration = WindowsIntegration::new(r"C:\Tools\TermCraft.exe").unwrap();
    let mut registry = MemoryRegistry::default();
    let cancel = AtomicBool::new(false);
    registry.put_unrelated("Software\\Classes\\unrelated", "keep");
    integration.install_into(&mut registry).unwrap();
    assert!(integration.uninstall_guard_in(&registry, &cancel).is_err());
    registry.put_unrelated(
        "Software\\Classes\\termcraft\\shell\\open\\command",
        "edited",
    );
    assert!(integration.uninstall_guard_in(&registry, &cancel).is_err());
    let mut registry = MemoryRegistry::default();
    integration.install_into(&mut registry).unwrap();
    integration.remove_from(&mut registry).unwrap();
    registry.put_unrelated("Software\\Classes\\termcraft", "other application");
    assert!(integration.uninstall_guard_in(&registry, &cancel).is_ok());
}
#[test]
fn cancellation_during_preflight_prevents_all_later_registry_mutations() {
    use harbor_workspace::platform::{
        registry_fs::RegistryValue, windows_integration::RegistryStore,
    };
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct CancelStore<'a> {
        cancel: &'a AtomicBool,
        mutations: usize,
    }
    impl RegistryStore for CancelStore<'_> {
        fn snapshot(&self, _: &str) -> Result<Option<BTreeMap<String, RegistryValue>>, String> {
            self.cancel.store(true, Ordering::Relaxed);
            Ok(None)
        }
        fn create_exclusive(
            &mut self,
            _: &str,
            _: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            self.mutations += 1;
            Ok(())
        }
        fn remove_exact(
            &mut self,
            _: &str,
            _: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            self.mutations += 1;
            Ok(())
        }
    }
    let cancel = AtomicBool::new(false);
    let mut store = CancelStore {
        cancel: &cancel,
        mutations: 0,
    };
    let result = WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
        .unwrap()
        .install_into_cancellable(&mut store, &cancel);
    assert!(result.is_err());
    assert_eq!(store.mutations, 0);
    cancel.store(false, Ordering::Relaxed);
    assert!(
        WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
            .unwrap()
            .remove_from_cancellable(&mut store, &cancel)
            .is_err()
    );
    assert_eq!(store.mutations, 0);
    cancel.store(false, Ordering::Relaxed);
    assert!(
        WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
            .unwrap()
            .status_in_cancellable(&store, &cancel)
            .is_err()
    );
}

#[cfg(windows)]
#[test]
fn native_shell_expansion_cannot_inject_workspace_options() {
    use harbor_workspace::cli::Cli;
    use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
    let mut registry = MemoryRegistry::default();
    WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
        .unwrap()
        .install_into(&mut registry)
        .unwrap();
    let template = registry
        .value("Software\\Classes\\termcraft\\shell\\open\\command")
        .unwrap()
        .display();
    for payload in [
        r#"termcraft://session/id" --data-dir "C:\attacker"#,
        r#"termcraft://session/id" --command "calc"#,
        r#"termcraft://session/id" --open-url "termcraft://session/other"#,
    ] {
        let command = template.replace("%1", payload);
        let wide: Vec<u16> = command.encode_utf16().chain([0]).collect();
        let mut count = 0;
        let raw = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut count) };
        assert!(!raw.is_null());
        let args: Vec<String> = unsafe { std::slice::from_raw_parts(raw, count as usize) }
            .iter()
            .map(|p| {
                let mut len = 0;
                unsafe {
                    while *p.add(len) != 0 {
                        len += 1;
                    }
                    String::from_utf16(std::slice::from_raw_parts(*p, len)).unwrap()
                }
            })
            .collect();
        unsafe {
            LocalFree(raw.cast());
        }
        assert_eq!(args[1], "--windows-activation-url");
        assert!(Cli::parse_invocation(args.into_iter().skip(1)).is_err());
    }
}

#[test]
fn cancellation_after_creation_stops_remaining_mutations_without_deleting_completed_entries() {
    use harbor_workspace::platform::{
        registry_fs::RegistryValue, windows_integration::RegistryStore,
    };
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct CancelAfterCreate<'a> {
        cancel: &'a AtomicBool,
        inner: MemoryRegistry,
        creates: usize,
        deletes: usize,
    }
    impl RegistryStore for CancelAfterCreate<'_> {
        fn snapshot(&self, key: &str) -> Result<Option<BTreeMap<String, RegistryValue>>, String> {
            self.inner.snapshot(key)
        }
        fn create_exclusive(
            &mut self,
            key: &str,
            values: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            self.inner.create_exclusive(key, values)?;
            self.creates += 1;
            self.cancel.store(true, Ordering::Relaxed);
            Ok(())
        }
        fn remove_exact(
            &mut self,
            key: &str,
            values: &BTreeMap<String, RegistryValue>,
        ) -> Result<(), String> {
            self.deletes += 1;
            self.inner.remove_exact(key, values)
        }
    }
    let cancel = AtomicBool::new(false);
    let mut store = CancelAfterCreate {
        cancel: &cancel,
        inner: MemoryRegistry::default(),
        creates: 0,
        deletes: 0,
    };
    assert!(
        WindowsIntegration::new(r"C:\Tools\TermCraft.exe")
            .unwrap()
            .install_into_cancellable(&mut store, &cancel)
            .is_err()
    );
    assert_eq!(store.creates, 1);
    assert_eq!(store.deletes, 0);
    assert!(
        store
            .inner
            .value("Software\\Classes\\Directory\\shell\\TermCraft.OpenHere")
            .is_some()
    );
}
