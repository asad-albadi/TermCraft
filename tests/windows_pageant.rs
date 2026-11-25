#![cfg(windows)]

use harbor_workspace::credentials::{
    agent::{AgentManager, AgentTarget},
    keys::{KeyManager, KeySettings},
};
use russh::keys::{HashAlg, PublicKey, ssh_key::SshSig};
use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

fn pageant_window_pid() -> Option<u32> {
    let name: Vec<u16> = "Pageant\0".encode_utf16().collect();
    let window = unsafe { FindWindowW(name.as_ptr(), name.as_ptr()) };
    if window.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    (pid != 0).then_some(pid)
}

struct Fixture(Child);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "requires a verified standalone Pageant executable on an isolated Windows desktop"]
fn pageant_add_list_and_remove_use_only_the_fixture() {
    let executable = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_PAGEANT_EXE").unwrap())
        .canonicalize()
        .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    assert!(
        pageant_window_pid().is_none(),
        "close the running Pageant before testing"
    );
    let mut fixture = Fixture(
        Command::new(executable)
            .current_dir(scratch.path())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while pageant_window_pid().is_none() {
        assert!(fixture.0.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "fixture did not publish its agent window"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let pid = fixture.0.id();
    assert_eq!(pageant_window_pid(), Some(pid));
    let pair = KeyManager::generate(
        &scratch.path().join("qualification key"),
        KeySettings::default(),
    )
    .unwrap();
    let manager = AgentManager::new(None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let target = AgentTarget::Pageant;
        assert!(manager.list(target).await.unwrap().identities.is_empty());
        assert_eq!(pageant_window_pid(), Some(pid));
        let added = manager
            .add(target, &pair.private, None, None)
            .await
            .unwrap();
        let listing = manager.list(target).await.unwrap();
        assert_eq!(listing.identities.len(), 1);
        assert_eq!(listing.identities[0].fingerprint, added.fingerprint);
        assert_eq!(pageant_window_pid(), Some(pid));
        let public = PublicKey::from_openssh(&added.public_key).unwrap();
        let data = b"TermCraft native Pageant qualification";
        let namespace = "termcraft-pageant-fixture";
        let message = SshSig::signed_data(namespace, HashAlg::Sha256, data).unwrap();
        let mut agent = manager.endpoint(target).unwrap().connect().await.unwrap();
        let signature = agent
            .sign_request_signature(&public, None, &message)
            .await
            .unwrap();
        let signed = SshSig::new(
            public.key_data().clone(),
            namespace,
            HashAlg::Sha256,
            signature,
        )
        .unwrap();
        public.verify(namespace, data, &signed).unwrap();
        manager.remove(target, &added.public_key).await.unwrap();
        assert!(manager.list(target).await.unwrap().identities.is_empty());
        assert!(
            agent
                .sign_request_signature(&public, None, &message)
                .await
                .is_err()
        );
    });
    drop(fixture);
    assert!(pageant_window_pid().is_none());
}
