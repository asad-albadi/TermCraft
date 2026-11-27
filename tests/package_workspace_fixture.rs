//! Synthetic-only qualification data. Verify never creates or repairs original files.
use harbor_workspace::{
    credentials::{Secret, vault::Vault},
    model::{Appearance, Session, Store},
    workspace::shortcuts::{Action, KeyChord},
};
use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

const CREDENTIAL: &str = "package-lifecycle-credential";
const SESSION: &str = "package-lifecycle-offline-profile";
const DRAFT: &str = "remote-recovery/qualification/draft.txt";
const CANARY: &str = "unrelated.txt";

fn contains_plaintext(root: &Path, values: [&[u8]; 2]) -> bool {
    let overlap = values.iter().map(|value| value.len()).max().unwrap() - 1;
    let mut pending = vec![root.to_path_buf()];
    let mut buffer = vec![0; 65536 + overlap];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("Cannot inspect fixture directory") {
            let entry = entry.expect("Cannot inspect workspace entry");
            let kind = entry
                .file_type()
                .expect("Cannot inspect workspace file type");
            assert!(!kind.is_symlink(), "Unexpected symbolic link in fixture");
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            assert!(kind.is_file(), "Unexpected nonregular fixture entry");
            let mut file =
                fs::File::open(entry.path()).expect("Cannot inspect persistent fixture file");
            let mut tail = 0;
            loop {
                let count = file
                    .read(&mut buffer[tail..])
                    .expect("Cannot read persistent fixture file");
                if count == 0 {
                    break;
                }
                let end = tail + count;
                if values.iter().any(|value| {
                    buffer[..end]
                        .windows(value.len())
                        .any(|part| part == *value)
                }) {
                    return true;
                }
                tail = overlap.min(end);
                buffer.copy_within(end - tail..end, 0);
            }
        }
    }
    false
}

#[test]
#[ignore = "requires an exclusively owned native package qualification directory"]
fn qualification_workspace() {
    // Delivered only over the private fixture process input pipe.
    let [master, marker]: [String; 2] = serde_json::from_reader(io::stdin().lock())
        .unwrap_or_else(|_| panic!("Missing or invalid private fixture input"));
    assert!(
        master != marker
            && [&master, &marker].iter().all(
                |value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            ),
        "Invalid private fixture input"
    );
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
            drop(Vault::create(&corrupt_vault, Secret::new(master.as_str())).unwrap());
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
            let mut vault = Vault::create(&encrypted, Secret::new(master.as_str())).unwrap();
            vault
                .put(CREDENTIAL.into(), Secret::new(marker.as_str()))
                .unwrap();
        }
        "verify" => (),
        _ => panic!("Expected explicit seed or verify"),
    }
    for original in [
        &saved,
        &encrypted,
        &corrupt.join("workspace.json"),
        &corrupt.join("credentials.vault"),
    ] {
        assert!(
            fs::symlink_metadata(original).is_ok_and(|metadata| metadata.is_file()),
            "Original fixture missing or not a regular file"
        );
    }
    for workspace in [&good, &corrupt] {
        assert!(
            fs::symlink_metadata(workspace).is_ok_and(|metadata| metadata.is_dir()),
            "Fixture workspace missing or not a directory"
        );
        assert!(
            !contains_plaintext(workspace, [master.as_bytes(), marker.as_bytes()]),
            "Plaintext fixture credential in persistent workspace"
        );
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
        Vault::unlock(
            &corrupt.join("credentials.vault"),
            Secret::new(master.as_str())
        )
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
    let vault = Vault::unlock(&encrypted, Secret::new(master.as_str())).unwrap();
    let stored = vault
        .get(&CREDENTIAL.into())
        .expect("Fixture credential missing");
    assert!(
        stored.expose() == marker,
        "Decrypted fixture credential did not match"
    );
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

#[test]
fn scan_detects_either_pattern_in_nested_sidecars_across_chunks() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("remote-recovery/one/nested");
    fs::create_dir_all(&directory).unwrap();
    let sidecar = directory.join("backup.sidecar");
    // Scanner vectors only, never used as the live qualification input.
    let (first, second) = ([b'A'; 64], [b'B'; 64]);
    let patterns: [&[u8]; 2] = [&first, &second];
    fs::write(&sidecar, b"unrelated data").unwrap();
    assert!(!contains_plaintext(root.path(), patterns));
    for pattern in patterns {
        // The first read can hold 65,536 bytes plus 63 bytes of overlap.
        // Split this pattern evenly across that 65,599-byte boundary.
        let mut bytes = vec![b'X'; 65_599 - pattern.len() / 2];
        bytes.extend_from_slice(pattern);
        fs::write(&sidecar, bytes).unwrap();
        assert!(contains_plaintext(root.path(), patterns));
    }
}
