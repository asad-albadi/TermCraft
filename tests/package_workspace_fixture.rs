//! Synthetic-only qualification data. Verify never creates or repairs original files.
use harbor_workspace::{
    credentials::{Secret, vault::Vault},
    model::{Appearance, Session, Store},
    workspace::shortcuts::{Action, KeyChord},
};
use std::{env, fs, path::PathBuf};

const MASTER: &str = "synthetic package lifecycle master, not a user credential";
const SECRET: &str = "TERMCRAFT_SYNTHETIC_PACKAGE_CREDENTIAL";
const CREDENTIAL: &str = "package-lifecycle-credential";
const SESSION: &str = "package-lifecycle-offline-profile";
const DRAFT: &str = "remote-recovery/qualification/draft.txt";
const CANARY: &str = "unrelated.txt";

#[test]
#[ignore = "requires an exclusively owned native package qualification directory"]
fn qualification_workspace() {
    let root = PathBuf::from(env::var_os("PACKAGE_FIXTURE_ROOT").expect("fixture root"));
    assert!(root.is_absolute() && root.is_dir());
    let good = root.join("workspace");
    let corrupt = root.join("malformed workspace");
    let saved = good.join("workspace.json");
    let encrypted = good.join("credentials.vault");
    let chord = KeyChord::new("F6", false, false, false);
    match env::var("PACKAGE_FIXTURE_MODE").unwrap().as_str() {
        "seed" => {
            fs::create_dir(&good).unwrap();
            fs::create_dir(&corrupt).unwrap();
            fs::create_dir_all(good.join(DRAFT).parent().unwrap()).unwrap();
            fs::write(good.join(DRAFT), "Synthetic recovery draft").unwrap();
            fs::write(good.join(CANARY), "Unrelated user content").unwrap();
            fs::write(corrupt.join("workspace.json"), b"{malformed fixture").unwrap();
            let corrupt_vault = corrupt.join("credentials.vault");
            // Preserve the native owner-only file mode/DACL while corrupting the contents,
            // so verification exercises the format rejection rather than a permission error.
            drop(Vault::create(&corrupt_vault, Secret::new(MASTER)).unwrap());
            fs::write(&corrupt_vault, b"malformed fixture vault").unwrap();
            let mut store = Store::default();
            store.sessions.push(Session {
                id: SESSION.into(),
                name: "Keep this saved session".into(),
                host: "example.test".into(),
                credential_ref: Some(CREDENTIAL.into()),
                ..Default::default()
            });
            store.preferences.font_size = 19.0;
            store.preferences.appearance = Appearance::Dark;
            store
                .preferences
                .shortcuts
                .bind(Action::NewSession, chord.clone())
                .unwrap();
            store.save(&saved).unwrap();
            let mut vault = Vault::create(&encrypted, Secret::new(MASTER)).unwrap();
            vault.put(CREDENTIAL.into(), Secret::new(SECRET)).unwrap();
        }
        "verify" => (),
        _ => panic!("Expected explicit seed or verify"),
    }
    for file in [
        &saved,
        &encrypted,
        &corrupt.join("workspace.json"),
        &corrupt.join("credentials.vault"),
    ] {
        let bytes = fs::read(file).expect("original fixture missing");
        for plaintext in [MASTER, SECRET] {
            assert!(
                !bytes
                    .windows(plaintext.len())
                    .any(|part| part == plaintext.as_bytes())
            );
        }
    }
    assert_eq!(
        fs::read_to_string(good.join(DRAFT)).unwrap(),
        "Synthetic recovery draft"
    );
    assert_eq!(
        fs::read_to_string(good.join(CANARY)).unwrap(),
        "Unrelated user content"
    );
    assert!(
        Store::load(&corrupt.join("workspace.json"))
            .unwrap_err()
            .contains("The original file was kept")
    );

    assert_eq!(
        Vault::unlock(&corrupt.join("credentials.vault"), Secret::new(MASTER))
            .err()
            .as_deref(),
        Some("Invalid or oversized vault file.")
    );
    let store = Store::load(&saved).unwrap();
    assert_eq!(store.sessions.len(), 1);
    assert_eq!(store.sessions[0].id, SESSION);
    assert_eq!(store.sessions[0].name, "Keep this saved session");
    assert_eq!(store.sessions[0].host, "example.test");
    assert_eq!(
        store.sessions[0].credential_ref.as_deref(),
        Some(CREDENTIAL)
    );
    assert_eq!(store.preferences.font_size, 19.0);
    assert_eq!(store.preferences.appearance, Appearance::Dark);
    assert!(
        store
            .preferences
            .shortcuts
            .bindings
            .iter()
            .any(|binding| { binding.action == Action::NewSession && binding.chord == chord })
    );
    let vault = Vault::unlock(&encrypted, Secret::new(MASTER)).unwrap();
    assert_eq!(vault.get(&CREDENTIAL.into()).unwrap().expose(), SECRET);
    assert_eq!(
        Vault::unlock(&encrypted, Secret::new("synthetic wrong password"))
            .err()
            .as_deref(),
        Some("Could not unlock vault: wrong password or damaged file.")
    );
    println!(
        "Package fixture verified: store, shortcuts, recovery, malformed originals, vault plaintext exclusion and decryption."
    );
}
