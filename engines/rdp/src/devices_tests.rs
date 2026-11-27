use super::devices::Drives;
use crate::rdp_options::DriveMapping;
fn word(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn request(device: u32, file: u32, id: u32, major: u32, minor: u32, body: &[u8]) -> Vec<u8> {
    let mut p = vec![0x72, 0x44, 0x52, 0x49];
    for n in [device, file, id, major, minor] {
        word(&mut p, n);
    }
    p.extend(body);
    p
}
fn create(path: &str, access: u32, disposition: u32, options: u32) -> Vec<u8> {
    let name: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut b = Vec::new();
    word(&mut b, access);
    b.extend([0; 8]);
    for n in [0, 7, disposition, options, name.len() as u32] {
        word(&mut b, n);
    }
    b.extend(name);
    b
}
fn value(p: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(p[offset..offset + 4].try_into().unwrap())
}
fn open(backend: &mut Drives, path: &str, access: u32, disposition: u32) -> Vec<u8> {
    backend
        .process(&request(
            1,
            0,
            7,
            0,
            0,
            &create(path, access, disposition, 0),
        ))
        .unwrap()
}
fn backend(root: &std::path::Path, read_only: bool) -> Drives {
    Drives::new(&[DriveMapping {
        name: "Work".into(),
        path: root.into(),
        read_only,
    }])
    .unwrap()
}
#[test]
fn redirected_drive_reads_writes_and_retires_real_handles() {
    let root = tempfile::tempdir().unwrap();
    let mut b = backend(root.path(), false);
    let p = open(&mut b, "\\hello.txt", 0xc0000000, 2);
    assert_eq!(value(&p, 12), 0);
    assert_eq!(
        p[20], 0,
        "new-file response information must be FILE_SUPERSEDED"
    );
    let file = value(&p, 16);
    let mut body = Vec::new();
    word(&mut body, 5);
    body.extend(0u64.to_le_bytes());
    body.extend([0; 20]);
    body.extend(b"hello");
    let p = b.process(&request(1, file, 8, 4, 0, &body)).unwrap();
    assert_eq!(value(&p, 12), 0);
    assert_eq!(value(&p, 16), 5);
    assert_eq!(
        std::fs::read(root.path().join("hello.txt")).unwrap(),
        b"hello"
    );
    let mut body = Vec::new();
    word(&mut body, 5);
    body.extend(0u64.to_le_bytes());
    body.extend([0; 20]);
    let p = b.process(&request(1, file, 9, 3, 0, &body)).unwrap();
    assert_eq!(value(&p, 12), 0);
    assert_eq!(&p[20..], b"hello");
    let p = b.process(&request(1, file, 10, 2, 0, &[0; 32])).unwrap();
    assert_eq!(value(&p, 12), 0);
    let p = b.process(&request(1, file, 11, 3, 0, &body)).unwrap();
    assert_ne!(value(&p, 12), 0);
}
#[cfg(windows)]
#[test]
fn windows_redirected_drive_reports_missing_metadata_lookup_before_create() {
    let root = tempfile::tempdir().unwrap();
    let mut drives = backend(root.path(), false);
    let path = "\\new-file.txt\0";
    let lookup = open(&mut drives, path, 0x00100080, 1);
    assert_eq!(value(&lookup, 12), 0xc000000f);
    assert_eq!(value(&lookup, 16), 0);
    assert!(!root.path().join("new-file.txt").exists());

    let ordinary_open = open(&mut drives, path, 0x80000000, 1);
    assert_eq!(value(&ordinary_open, 12), 0xc0000034);
    let create_response = drives
        .process(&request(1, 0, 8, 0, 0, &create(path, 0x0016019f, 2, 0x44)))
        .unwrap();
    assert_eq!(value(&create_response, 12), 0);
    assert!(root.path().join("new-file.txt").is_file());
}
#[test]
fn redirected_read_only_drive_blocks_mutations_and_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("safe"), b"saved").unwrap();
    std::fs::write(outside.path().join("secret"), b"private").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    let mut b = backend(root.path(), true);
    for (path, access, disposition) in [
        ("\\new", 0x40000000, 2),
        ("\\safe", 0x40000000, 5),
        ("\\..\\secret", 0x80000000, 1),
        ("\\escape\\secret", 0x80000000, 1),
        ("\\safe:stream", 0x80000000, 1),
    ] {
        assert_ne!(
            value(&open(&mut b, path, access, disposition), 12),
            0,
            "{path}"
        );
    }
    assert_eq!(std::fs::read(root.path().join("safe")).unwrap(), b"saved");
    assert!(!root.path().join("new").exists());
    assert_eq!(value(&open(&mut b, "\\safe", 0x80000000, 1), 12), 0);
}
#[test]
fn redirected_drives_reject_malformed_pdus_and_bound_handles() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("safe"), b"ok").unwrap();
    let mut b = backend(root.path(), false);
    for len in 0..24 {
        assert!(b.process(&vec![0; len]).is_err());
    }
    let mut bad = create("\\safe", 0x80000000, 1, 0);
    bad[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_ne!(
        value(&b.process(&request(1, 0, 1, 0, 0, &bad)).unwrap(), 12),
        0
    );
    for _ in 0..128 {
        assert_eq!(value(&open(&mut b, "\\safe", 0x80000000, 1), 12), 0);
    }
    assert_ne!(value(&open(&mut b, "\\safe", 0x80000000, 1), 12), 0);
    assert_ne!(
        value(
            &b.process(&request(99, 0, 1, 0, 0, &create("safe", 0x80000000, 1, 0)))
                .unwrap(),
            12
        ),
        0
    );
}
#[test]
fn drive_channel_handshake_advertises_only_after_logon_and_processes_io() {
    use ironrdp_svc::SvcProcessor;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("test"), b"drive-content").unwrap();
    let mut channel = super::devices::Channel::new(&[DriveMapping {
        name: "Work".into(),
        path: root.path().into(),
        read_only: true,
    }])
    .unwrap();
    let messages = channel
        .process(&[0x72, 0x44, 0x6e, 0x49, 1, 0, 12, 0, 1, 0, 0, 0])
        .unwrap();
    assert_eq!(messages.len(), 2);
    let messages = channel
        .process(&[0x72, 0x44, 0x50, 0x53, 0, 0, 0, 0])
        .unwrap();
    let bytes = messages[0].encode_unframed_pdu().unwrap();
    assert_eq!(bytes.len(), 60);
    assert_eq!(value(&bytes, 4) & 0xffff, 2);
    assert!(
        channel
            .process(&request(1, 0, 1, 0, 0, &create("test", 0x80000000, 1, 0)))
            .is_err()
    );
    let messages = channel
        .process(&[0x72, 0x44, 0x43, 0x43, 1, 0, 12, 0, 1, 0, 0, 0])
        .unwrap();
    assert!(messages.is_empty());
    let messages = channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap();
    assert_eq!(messages.len(), 1);
    let bytes = messages[0].encode_unframed_pdu().unwrap();
    assert_eq!(value(&bytes, 4), 1);
    assert_eq!(value(&bytes, 8), 8);
    assert_eq!(&bytes[16..24], b"Work\0\0\0\0");
    assert!(
        channel
            .process(&[0x72, 0x44, 0x4c, 0x55])
            .unwrap()
            .is_empty()
    );
    channel
        .process(&request(1, 0, 1, 0, 0, &create("test", 0x80000000, 1, 0)))
        .unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let p = channel.poll().unwrap();
        if !p.is_empty() {
            let bytes = p[0].encode_unframed_pdu().unwrap();
            assert_eq!(value(&bytes, 12), 0);
            assert_ne!(value(&bytes, 16), 0);
            break;
        }
        assert!(std::time::Instant::now() < end);
        std::thread::yield_now();
    }
}
fn set_info(b: &mut Drives, file: u32, class: u32, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    word(&mut body, class);
    word(&mut body, data.len() as u32);
    body.resize(32, 0);
    body.extend(data);
    b.process(&request(1, file, 33, 6, 0, &body)).unwrap()
}
#[test]
fn redirected_folder_listing_uses_rdpefs_layout_and_never_loses_late_matches() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("penguin"), b"bird").unwrap();
    let mut b = backend(root.path(), false);
    let result = b
        .process(&request(1, 0, 1, 0, 0, &create("\\", 0x80000000, 1, 1)))
        .unwrap();
    assert_eq!(value(&result, 12), 0);
    let id = value(&result, 16);
    let mut query = Vec::new();
    word(&mut query, 3);
    query.push(1);
    let pattern: Vec<_> = "*".encode_utf16().flat_map(u16::to_le_bytes).collect();
    word(&mut query, pattern.len() as u32);
    query.resize(32, 0);
    query.extend(pattern);
    let result = b.process(&request(1, id, 2, 12, 1, &query)).unwrap();
    assert_eq!(value(&result, 12), 0);
    assert_eq!(value(&result, 16), 107);
    assert_eq!(
        &result[113..],
        &"penguin"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    );
    for i in 0..1200 {
        std::fs::write(root.path().join(format!("entry{i}")), b"data").unwrap();
    }
    let late = std::fs::read_dir(root.path())
        .unwrap()
        .nth(1100)
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    let pattern: Vec<_> = late.encode_utf16().flat_map(u16::to_le_bytes).collect();
    query.truncate(5);
    word(&mut query, pattern.len() as u32);
    query.resize(32, 0);
    query.extend(pattern);
    let result = b.process(&request(1, id, 3, 12, 1, &query)).unwrap();
    assert_eq!(
        value(&result, 12),
        0,
        "a matching late directory entry was lost"
    );
}
#[cfg(not(windows))]
#[test]
fn redirected_file_resize_preserves_unsupported_rename_and_delete() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source"), b"original").unwrap();
    std::fs::write(root.path().join("keep"), b"keep").unwrap();
    let mut b = backend(root.path(), false);
    let id = value(&open(&mut b, "source", 0xc0010000, 1), 16);
    assert_eq!(value(&set_info(&mut b, id, 20, &3u64.to_le_bytes()), 12), 0);
    assert_eq!(std::fs::read(root.path().join("source")).unwrap(), b"ori");
    let rename = |name: &str, replace: bool| {
        let name: Vec<_> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut data = vec![u8::from(replace), 0];
        word(&mut data, name.len() as u32);
        data.extend(name);
        data
    };
    assert_ne!(
        value(&set_info(&mut b, id, 10, &rename("keep", false)), 12),
        0
    );
    assert_eq!(std::fs::read(root.path().join("keep")).unwrap(), b"keep");
    assert_ne!(
        value(&set_info(&mut b, id, 10, &rename("moved", false)), 12),
        0
    );
    assert!(root.path().join("source").exists());
    assert_ne!(value(&set_info(&mut b, id, 13, &[]), 12), 0);
    assert_eq!(
        value(&b.process(&request(1, id, 44, 2, 0, &[0; 32])).unwrap(), 12),
        0
    );
    assert_eq!(std::fs::read(root.path().join("source")).unwrap(), b"ori");
}
#[cfg(windows)]
#[test]
fn redirected_windows_file_mutations_use_protocol_layout_and_keep_handle_identity() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source"), b"original").unwrap();
    let mut b = backend(root.path(), false);
    let opened = open(&mut b, "source", 0xc0010000, 1);
    assert_eq!(value(&opened, 12), 0);
    let id = value(&opened, 16);
    std::fs::rename(root.path().join("source"), root.path().join("local-name")).unwrap();
    std::fs::write(root.path().join("source"), b"replacement").unwrap();
    let name: Vec<_> = "remote-name"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut data = vec![0, 0];
    word(&mut data, name.len() as u32);
    data.extend(name);
    let mut invalid = data.clone();
    invalid[1] = 1;
    assert_eq!(value(&set_info(&mut b, id, 10, &invalid), 12), 0xc000000d);
    assert_eq!(value(&set_info(&mut b, id, 10, &data), 12), 0);
    assert_eq!(
        std::fs::read(root.path().join("remote-name")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(root.path().join("source")).unwrap(),
        b"replacement"
    );
    assert_eq!(value(&set_info(&mut b, id, 13, &[]), 12), 0);
    assert_eq!(
        value(&b.process(&request(1, id, 99, 2, 0, &[0; 32])).unwrap(), 12),
        0
    );
    assert!(!root.path().join("remote-name").exists());
    assert_eq!(
        std::fs::read(root.path().join("source")).unwrap(),
        b"replacement"
    );
}

#[cfg(unix)]
#[test]
fn redirected_volume_reports_actual_capacity() {
    let root = tempfile::tempdir().unwrap();
    let mut b = backend(root.path(), true);
    let id = value(
        &b.process(&request(1, 0, 1, 0, 0, &create("\\", 0x80000000, 1, 1)))
            .unwrap(),
        16,
    );
    let mut query = vec![0; 32];
    query[..4].copy_from_slice(&3u32.to_le_bytes());
    let response = b.process(&request(1, id, 2, 10, 0, &query)).unwrap();
    assert_eq!(value(&response, 12), 0);
    assert_eq!(value(&response, 16), 24);
    assert!(u64::from_le_bytes(response[20..28].try_into().unwrap()) > 0);
    assert!(value(&response, 36) > 0);
    assert_eq!(value(&response, 40), 512);
}
#[test]
fn drive_handles_enforce_requested_access_and_exclusive_sharing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"original").unwrap();
    let mut b = backend(root.path(), false);
    let metadata = value(&open(&mut b, "file", 0x80, 1), 16);
    let mut body = vec![0; 32];
    body[..4].copy_from_slice(&3u32.to_le_bytes());
    assert_ne!(
        value(
            &b.process(&request(1, metadata, 1, 3, 0, &body)).unwrap(),
            12
        ),
        0,
        "metadata-only handle read file data"
    );
    let append = value(&open(&mut b, "file", 4, 1), 16);
    body.extend(b"new");
    assert_ne!(
        value(&b.process(&request(1, append, 2, 4, 0, &body)).unwrap(), 12),
        0,
        "append-only handle overwrote offset zero"
    );
    let mut exclusive = create("file", 0xc0000000, 1, 0);
    exclusive[16..20].copy_from_slice(&0u32.to_le_bytes());
    let response = b.process(&request(1, 0, 3, 0, 0, &exclusive)).unwrap();
    assert_ne!(
        value(&response, 12),
        0,
        "exclusive handle ignored existing shared readers"
    );
    b.process(&request(1, metadata, 4, 2, 0, &[0; 32])).unwrap();
    b.process(&request(1, append, 5, 2, 0, &[0; 32])).unwrap();
    let response = b.process(&request(1, 0, 6, 0, 0, &exclusive)).unwrap();
    assert_eq!(value(&response, 12), 0);
    let exclusive_id = value(&response, 16);
    assert_ne!(
        value(&open(&mut b, "file", 0x40000000, 5), 12),
        0,
        "exclusive share allowed another handle to truncate data"
    );
    let closed = b
        .process(&request(1, exclusive_id, 7, 2, 0, &[0; 32]))
        .unwrap();
    assert_eq!(value(&closed, 12), 0);
    assert_eq!(
        std::fs::read(root.path().join("file")).unwrap(),
        b"original"
    );
}
#[test]
fn unsupported_drive_mutations_never_change_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"original").unwrap();
    let mut b = backend(root.path(), false);
    let response = b
        .process(&request(
            1,
            0,
            1,
            0,
            0,
            &create("file", 0xc0010000, 5, 0x1000),
        ))
        .unwrap();
    assert_ne!(
        value(&response, 12),
        0,
        "delete-on-close must fail before truncating"
    );
    assert_eq!(
        std::fs::read(root.path().join("file")).unwrap(),
        b"original"
    );
    let id = value(&open(&mut b, "file", 0xc0010000, 1), 16);
    // Windows supports a zero-length disposition request. Use malformed
    // requests to verify that rejected mutations leave the contents intact.
    for (class, data) in [(13, vec![0]), (10, vec![0; 6])] {
        let mut body = Vec::new();
        word(&mut body, class);
        word(&mut body, data.len() as u32);
        body.resize(32, 0);
        body.extend(data);
        assert_ne!(
            value(&b.process(&request(1, id, 2, 6, 0, &body)).unwrap(), 12),
            0,
            "unsupported mutation {class} falsely succeeded"
        );
    }
    assert_eq!(
        std::fs::read(root.path().join("file")).unwrap(),
        b"original"
    );
    assert_ne!(
        value(&b.process(&request(1, id, 3, 14, 0, &[0; 32])).unwrap(), 12),
        0,
        "unknown IOCTL falsely succeeded"
    );
}
#[test]
fn drive_announcements_use_unicode_v2_names_and_saturated_io_replies_without_disconnect() {
    use ironrdp_svc::SvcProcessor;
    let root = tempfile::tempdir().unwrap();
    let mappings = [DriveMapping {
        name: "Work".into(),
        path: root.path().into(),
        read_only: true,
    }];
    let announce = super::devices::announce_mappings(&mappings);
    assert_eq!(value(&announce, 24), 10);
    assert_eq!(&announce[28..], b"W\0o\0r\0k\0\0\0");
    let mut channel = super::devices::Channel::new(&mappings).unwrap();
    channel
        .process(&[0x72, 0x44, 0x6e, 0x49, 1, 0, 12, 0, 1, 0, 0, 0])
        .unwrap();
    channel
        .process(&[0x72, 0x44, 0x43, 0x43, 1, 0, 12, 0, 1, 0, 0, 0])
        .unwrap();
    channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap();
    for id in 0..16 {
        channel
            .process(&request(
                1,
                0,
                id,
                0,
                0,
                &create("absent", 0x80000000, 1, 0),
            ))
            .unwrap();
    }
    let messages = channel
        .process(&request(
            1,
            0,
            99,
            0,
            0,
            &create("absent", 0x80000000, 1, 0),
        ))
        .expect("queue saturation disconnected the desktop");
    assert_eq!(messages.len(), 1);
    let p = messages[0].encode_unframed_pdu().unwrap();
    assert_eq!(value(&p, 8), 99);
    assert_ne!(value(&p, 12), 0);
}
#[test]
fn opened_drive_handles_keep_the_original_object_after_a_local_rename() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"original").unwrap();
    let mut b = backend(root.path(), false);
    let id = value(&open(&mut b, "file", 0x80000000, 1), 16);
    std::fs::rename(root.path().join("file"), root.path().join("old")).unwrap();
    std::fs::write(root.path().join("file"), b"new").unwrap();
    let mut query = vec![0; 32];
    query[..4].copy_from_slice(&5u32.to_le_bytes());
    let reply = b.process(&request(1, id, 4, 5, 0, &query)).unwrap();
    assert_eq!(value(&reply, 12), 0);
    assert_eq!(u64::from_le_bytes(reply[28..36].try_into().unwrap()), 8);
    let mut body = vec![0; 32];
    body[..4].copy_from_slice(&8u32.to_le_bytes());
    let reply = b.process(&request(1, id, 5, 3, 0, &body)).unwrap();
    assert_eq!(&reply[20..], b"original");
    assert_eq!(std::fs::read(root.path().join("file")).unwrap(), b"new");
}
#[cfg(unix)]
#[test]
fn redirected_drives_reject_fifos_without_opening_a_blocking_stream() {
    let root = tempfile::tempdir().unwrap();
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(root.path().join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let mut b = backend(root.path(), false);
    assert_ne!(value(&open(&mut b, "fifo", 0x80000000, 1), 12), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn allocation_reserves_space_without_extending_eof_and_validates_signed_lengths() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file"), b"original").unwrap();
    let mut b = backend(root.path(), false);
    let id = value(&open(&mut b, "file", 0xc0000000, 1), 16);
    let response = set_info(&mut b, id, 19, &65536u64.to_le_bytes());
    assert_eq!(value(&response, 12), 0, "allocation request failed");
    let metadata = std::fs::metadata(root.path().join("file")).unwrap();
    assert_eq!(metadata.len(), 8, "allocation changed logical EOF");
    let mut query = vec![0; 32];
    query[..4].copy_from_slice(&5u32.to_le_bytes());
    let result = b.process(&request(1, id, 77, 5, 0, &query)).unwrap();
    assert!(
        u64::from_le_bytes(result[20..28].try_into().unwrap()) >= 65536,
        "file information hid actual reserved storage"
    );
    assert!(metadata.blocks() * 512 >= 65536, "space was not reserved");
    assert_ne!(
        value(&set_info(&mut b, id, 20, &u64::MAX.to_le_bytes()), 12),
        0
    );
    assert_ne!(
        value(&set_info(&mut b, id, 19, &u64::MAX.to_le_bytes()), 12),
        0
    );
    assert_eq!(
        std::fs::read(root.path().join("file")).unwrap(),
        b"original"
    );
    assert_eq!(value(&set_info(&mut b, id, 19, &3u64.to_le_bytes()), 12), 0);
    assert_eq!(std::fs::read(root.path().join("file")).unwrap(), b"ori");
    let read = value(&open(&mut b, "file", 0x80000000, 1), 16);
    assert_ne!(
        value(&set_info(&mut b, read, 19, &4096u64.to_le_bytes()), 12),
        0
    );
}

#[cfg(target_os = "linux")]
#[test]
fn drive_allocation_is_released_on_last_remote_close_and_disconnect() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");
    std::fs::write(&path, b"original").unwrap();
    for disconnect in [false, true] {
        let mut b = backend(root.path(), false);
        let id = value(&open(&mut b, "file", 0xc0000000, 1), 16);
        let other = value(&open(&mut b, "file", 0xc0000000, 1), 16);
        assert_eq!(
            value(&set_info(&mut b, id, 19, &1048576u64.to_le_bytes()), 12),
            0
        );
        b.process(&request(1, id, 1, 2, 0, &[0; 32])).unwrap();
        assert!(
            std::fs::metadata(&path).unwrap().blocks() * 512 >= 1048576,
            "released before final handle closed"
        );
        if !disconnect {
            assert_eq!(
                value(
                    &b.process(&request(1, other, 2, 2, 0, &[0; 32])).unwrap(),
                    12
                ),
                0
            );
        }
        drop(b);
        assert!(
            std::fs::metadata(&path).unwrap().blocks() * 512 < 1048576,
            "unused remote allocation leaked after close"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn drive_allocation_release_preserves_data_written_into_the_reservation() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");
    std::fs::write(&path, b"original").unwrap();
    let mut b = backend(root.path(), false);
    let id = value(&open(&mut b, "file", 0xc0000000, 1), 16);
    assert_eq!(
        value(&set_info(&mut b, id, 19, &1048576u64.to_le_bytes()), 12),
        0
    );
    // Grow EOF into the reservation, ending in the middle of a filesystem block.
    let mut body = Vec::new();
    word(&mut body, 3);
    body.extend(8192u64.to_le_bytes());
    body.extend([0; 20]);
    body.extend(b"end");
    assert_eq!(
        value(&b.process(&request(1, id, 2, 4, 0, &body)).unwrap(), 12),
        0
    );
    assert!(std::fs::metadata(&path).unwrap().blocks() * 512 >= 1048576);
    assert_eq!(
        value(&b.process(&request(1, id, 3, 2, 0, &[0; 32])).unwrap(), 12),
        0
    );
    let metadata = std::fs::metadata(&path).unwrap();
    assert_eq!(metadata.len(), 8195, "release changed logical EOF");
    assert!(
        metadata.blocks() * 512 < 1048576,
        "unused remote allocation leaked after writing and closing"
    );
    let mut expected = b"original".to_vec();
    expected.resize(8192, 0);
    expected.extend(b"end");
    assert_eq!(std::fs::read(&path).unwrap(), expected);
}

#[cfg(target_os = "linux")]
pub(crate) mod serial {
    use super::*;
    use crate::{
        devices::{BoundSerialMappings, Channel},
        rdp_options::SerialMapping,
    };
    use ironrdp_svc::SvcProcessor;
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::{AsRawFd, FromRawFd},
        path::PathBuf,
        time::{Duration, Instant},
    };

    pub(crate) fn pty() -> (File, File, PathBuf) {
        let (mut master, mut slave) = (-1, -1);
        let mut path = [0 as libc::c_char; 128];
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    path.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let path = unsafe { std::ffi::CStr::from_ptr(path.as_ptr()) };
        let path = PathBuf::from(path.to_str().unwrap());
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            0
        );
        (master, slave, path)
    }

    fn start(path: PathBuf, read_only: bool) -> Channel {
        handshake(
            Channel::with_serial(
                &[],
                &[SerialMapping {
                    name: "com2".into(),
                    path,
                    read_only,
                }],
            )
            .unwrap(),
        )
    }

    fn handshake(mut channel: Channel) -> Channel {
        channel
            .process(&[0x72, 0x44, 0x6e, 0x49, 1, 0, 12, 0, 1, 0, 0, 0])
            .unwrap();
        let caps = channel
            .process(&[0x72, 0x44, 0x50, 0x53, 0, 0, 0, 0])
            .unwrap()[0]
            .encode_unframed_pdu()
            .unwrap();
        assert_eq!(&caps[52..], &[3, 0, 8, 0, 1, 0, 0, 0]);
        assert!(
            channel
                .process(&[0x72, 0x44, 0x43, 0x43, 1, 0, 12, 0, 1, 0, 0, 0])
                .unwrap()
                .is_empty()
        );
        let announce = channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap()[0]
            .encode_unframed_pdu()
            .unwrap();
        assert_eq!(
            &announce[..16],
            &[114, 68, 65, 68, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]
        );
        assert_eq!(&announce[16..], b"COM2\0\0\0\0\0\0\0\0");
        channel
    }

    fn send(channel: &mut Channel, handle: u32, id: u32, major: u32, body: &[u8]) {
        assert!(
            channel
                .process(&request(1, handle, id, major, 0, body))
                .unwrap()
                .is_empty()
        );
    }

    fn completions(channel: &mut Channel, count: usize) -> Vec<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut packets = Vec::new();
        while packets.len() < count {
            packets.extend(
                channel
                    .poll()
                    .unwrap()
                    .iter()
                    .map(|p| p.encode_unframed_pdu().unwrap()),
            );
            assert!(Instant::now() < deadline, "missing serial completions");
            std::thread::sleep(Duration::from_millis(1));
        }
        packets
    }

    fn ioctl(code: u32, input: &[u8], output: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for n in [output, input.len() as u32, code] {
            word(&mut b, n);
        }
        b.extend([0; 20]);
        b.extend(input);
        b
    }

    #[test]
    fn unix_event_saturated_channel_allows_mask_reset_and_close() {
        let (_master, _slave, path) = pty();
        let mut channel = start(path, false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let h = value(&completions(&mut channel, 1)[0], 16);
        // Zero total read timeout denotes an indefinite read.
        send(&mut channel, h, 2, 14, &ioctl(0x1b001c, &[0; 20], 0));
        send(
            &mut channel,
            h,
            3,
            14,
            &ioctl(0x1b0044, &1u32.to_le_bytes(), 0),
        );
        for packet in completions(&mut channel, 2) {
            assert_eq!(value(&packet, 12), 0);
        }
        let mut read = [0u8; 32];
        read[..4].copy_from_slice(&1u32.to_le_bytes());
        for seq in 4..19 {
            send(&mut channel, h, seq, 3, &read);
        }
        send(&mut channel, h, 19, 14, &ioctl(0x1b0048, &[], 4));
        let busy = channel
            .process(&request(1, h, 20, 14, 0, &ioctl(0x1b0040, &[], 4)))
            .unwrap();
        assert_eq!(
            value(&busy[0].encode_unframed_pdu().unwrap(), 12),
            0x80000011
        );
        // Malformed requests and unmapped devices cannot spend the reset slot.
        for (dev, minor, control) in [
            (1, 0, ioctl(0x1b0044, &[0; 3], 0)),
            (2, 0, ioctl(0x1b0044, &[0; 4], 0)),
            (1, 1, ioctl(0x1b0044, &[0; 4], 0)),
        ] {
            let busy = channel
                .process(&request(dev, h, 100, 14, minor, &control))
                .unwrap();
            assert_eq!(
                value(&busy[0].encode_unframed_pdu().unwrap(), 12),
                0x80000011
            );
        }
        send(
            &mut channel,
            h,
            21,
            14,
            &ioctl(0x1b0044, &0u32.to_le_bytes(), 0),
        );
        let busy = channel
            .process(&request(1, h, 22, 14, 0, &ioctl(0x1b0044, &[0; 4], 0)))
            .unwrap();
        assert_eq!(
            value(&busy[0].encode_unframed_pdu().unwrap(), 12),
            0x80000011
        );
        let cancelled_wait = |packets: &[Vec<u8>], id| {
            let packet = packets.iter().find(|p| value(p, 8) == id).unwrap();
            assert_eq!(value(packet, 12), 0);
            assert_eq!(&packet[16..], &[4, 0, 0, 0, 0, 0, 0, 0]);
        };
        let packets = completions(&mut channel, 2);
        cancelled_wait(&packets, 19);
        assert!(
            packets
                .iter()
                .any(|p| value(p, 8) == 21 && value(p, 12) == 0)
        );

        // The getter that was refused now works before closing, with all 15
        // reads still pending and the same handle and reset mask.
        send(&mut channel, h, 20, 14, &ioctl(0x1b0040, &[], 4));
        cancelled_wait(&completions(&mut channel, 1), 20);
        send(
            &mut channel,
            h,
            23,
            14,
            &ioctl(0x1b0044, &1u32.to_le_bytes(), 0),
        );
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        send(&mut channel, h, 24, 14, &ioctl(0x1b0048, &[], 4));
        send(&mut channel, h, 25, 14, &ioctl(0x1b0044, &[0; 4], 0));
        // Do not poll between reset and close. The close must fit as the 18th request.
        send(&mut channel, h, 26, 2, &[0; 32]);
        let packets = completions(&mut channel, 18);
        assert_eq!(packets.len(), 18);
        cancelled_wait(&packets, 24);
        for seq in (4..19).chain([25, 26]) {
            let packet = packets.iter().find(|p| value(p, 8) == seq).unwrap();
            assert_eq!(value(packet, 12), if seq < 19 { 0xc0000120 } else { 0 });
        }
        // Every ordinary credit is reusable after closing and reopening.
        send(&mut channel, 0, 27, 0, &create("", 0xc0000000, 1, 0));
        let reopened = value(&completions(&mut channel, 1)[0], 16);
        assert_ne!(h, reopened);
        for seq in 28..44 {
            send(&mut channel, reopened, seq, 14, &ioctl(0x1b0040, &[], 4));
        }
        for packet in completions(&mut channel, 16) {
            assert_eq!(value(&packet, 12), 0);
        }
    }

    #[test]
    fn embedded_serial_redirects_an_owned_pty_with_concurrent_read_and_write() {
        let (mut master, slave, path) = pty();
        let mut channel = start(path, false);
        let original_speed = baud_rate(&slave);
        send(&mut channel, 0, 1, 0, &create("\\other", 0xc0000000, 1, 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc0000022);
        send(&mut channel, 0, 2, 0, &create("", 0xc0000000, 1, 0));
        let packets = completions(&mut channel, 1);
        assert_eq!(value(&packets[0], 12), 0);
        let completion = &packets[0];
        let handle = value(&completion, 16);
        send(
            &mut channel,
            handle,
            3,
            14,
            &ioctl(0x001b0004, &19200u32.to_le_bytes(), 0),
        );
        send(&mut channel, handle, 4, 14, &ioctl(0x001b0050, &[], 4));
        let packets = completions(&mut channel, 2);
        assert_eq!(value(&packets[0], 12), 0);
        assert_eq!(value(&packets[1], 20), 19200);
        assert_eq!(baud_rate(&slave), libc::B19200);
        send(
            &mut channel,
            handle,
            30,
            14,
            &ioctl(0x001b000c, &[0, 0, 8], 0),
        );
        send(&mut channel, handle, 31, 14, &ioctl(0x001b0054, &[], 3));
        let packets = completions(&mut channel, 2);
        assert_eq!(value(&packets[0], 12), 0);
        assert_eq!(&packets[1][20..], &[0, 0, 8]);

        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&4u32.to_le_bytes());
        send(&mut channel, handle, 5, 3, &transfer);
        transfer.extend(b"send");
        send(&mut channel, handle, 6, 4, &transfer);
        let completion = completions(&mut channel, 1).pop().unwrap();
        assert_eq!(value(&completion, 8), 6);
        assert_eq!(value(&completion, 16), 4);
        let mut buffer = [0; 4];
        master.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"send");
        master.write_all(b"recv").unwrap();
        let completion = completions(&mut channel, 1).pop().unwrap();
        assert_eq!(value(&completion, 8), 5);
        assert_eq!(&completion[20..], b"recv");

        send(&mut channel, handle, 7, 3, &transfer[..32]);
        send(&mut channel, handle, 8, 2, &[0; 32]);
        let mut packets = completions(&mut channel, 2);
        packets.sort_by_key(|p| value(p, 8));
        assert_eq!(value(&packets[0], 12), 0xc0000120);
        assert_eq!(value(&packets[1], 12), 0);
        assert_eq!(baud_rate(&slave), original_speed);
    }

    fn baud_rate(slave: &File) -> libc::speed_t {
        let mut line = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), line.as_mut_ptr()) },
            0
        );
        unsafe { libc::cfgetospeed(line.as_ptr()) }
    }

    #[test]
    fn embedded_serial_close_cancels_all_waiting_reads_even_at_capacity() {
        let (_master, _slave, path) = pty();
        let mut channel = start(path, false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let packets = completions(&mut channel, 1);
        let handle = value(&packets[0], 16);
        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&1u32.to_le_bytes());
        for id in 2..18 {
            send(&mut channel, handle, id, 3, &transfer);
        }
        let busy = channel
            .process(&request(1, handle, 18, 3, 0, &transfer))
            .unwrap();
        assert_eq!(
            value(&busy[0].encode_unframed_pdu().unwrap(), 12),
            0x80000011
        );
        send(&mut channel, handle, 19, 2, &[0; 32]);
        let mut packets = completions(&mut channel, 17);
        packets.sort_by_key(|p| value(p, 8));
        assert!(packets[..16].iter().all(|p| value(p, 12) == 0xc0000120));
        assert_eq!(value(&packets[16], 8), 19);
        assert_eq!(value(&packets[16], 12), 0);
    }

    #[test]
    fn embedded_serial_opens_the_specification_metadata_probe() {
        let (_master, _slave, path) = pty();
        let mut channel = start(path, true);
        send(&mut channel, 0, 1, 0, &create("", 0x00100080, 1, 0x60));
        let packets = completions(&mut channel, 1);
        assert_eq!(value(&packets[0], 12), 0);
        assert_eq!(packets[0][20], 0);
        let handle = value(&packets[0], 16);
        send(&mut channel, handle, 2, 3, &[0; 32]);
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc0000022);
    }

    #[test]
    fn serial_configured_timeouts_accumulate_bytes_and_support_immediate_reads() {
        let (mut master, _slave, path) = pty();
        let mut channel = start(path, false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let handle = value(&completions(&mut channel, 1)[0], 16);
        let settings: Vec<u8> = [25u32, 0, 250, 0, 250]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        send(
            &mut channel,
            handle,
            2,
            14,
            &ioctl(0x001b001c, &settings, 0),
        );
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        send(&mut channel, handle, 3, 14, &ioctl(0x001b0020, &[], 20));
        assert_eq!(&completions(&mut channel, 1)[0][20..], settings);
        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&4u32.to_le_bytes());
        send(&mut channel, handle, 4, 3, &transfer);
        master.write_all(b"ab").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            channel.poll().unwrap().is_empty(),
            "partial read must wait for the configured interval"
        );
        let partial = completions(&mut channel, 1).pop().unwrap();
        assert_eq!(&partial[20..], b"ab");
        let immediate: Vec<u8> = [u32::MAX, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        send(
            &mut channel,
            handle,
            5,
            14,
            &ioctl(0x001b001c, &immediate, 0),
        );
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        send(&mut channel, handle, 6, 3, &transfer);
        assert_eq!(&completions(&mut channel, 1)[0][16..], &[0; 4]);
        master.write_all(b"xy").unwrap();
        send(&mut channel, handle, 7, 3, &transfer);
        assert_eq!(&completions(&mut channel, 1)[0][20..], b"xy");
    }

    #[test]
    fn serial_xoff_stalls_pty_output_until_xon_and_read_only_rejects_new_controls() {
        let (mut master, _slave, path) = pty();
        let mut channel = start(path, false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let handle = value(&completions(&mut channel, 1)[0], 16);
        send(&mut channel, handle, 2, 14, &ioctl(0x001b0038, &[], 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&4u32.to_le_bytes());
        transfer.extend(b"flow");
        send(&mut channel, handle, 3, 4, &transfer);
        std::thread::sleep(Duration::from_millis(10));
        assert!(channel.poll().unwrap().is_empty());
        assert_eq!(
            master.read(&mut [0; 4]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        send(&mut channel, handle, 4, 14, &ioctl(0x001b003c, &[], 0));
        let mut packets = completions(&mut channel, 2);
        packets.sort_by_key(|p| value(p, 8));
        assert!(packets.iter().all(|p| value(p, 12) == 0));
        let mut data = [0; 4];
        master.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"flow");
        drop(channel);
        let (_master, _slave, path) = pty();
        let mut channel = start(path, true);
        send(&mut channel, 0, 1, 0, &create("", 0x80000000, 1, 0));
        let handle = value(&completions(&mut channel, 1)[0], 16);
        for (code, bytes) in [
            (0x001b001c, vec![0; 20]),
            (0x001b0058, vec![0; 6]),
            (0x001b0064, vec![0; 16]),
            (0x001b0024, vec![]),
            (0x001b0030, vec![]),
            (0x001b0010, vec![]),
            (0x001b0038, vec![]),
        ] {
            send(&mut channel, handle, 2, 14, &ioctl(code, &bytes, 0));
            assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc0000022);
        }
    }

    #[test]
    fn serial_flow_characters_apply_to_pty_and_modem_failures_are_explicit() {
        let (_master, slave, path) = pty();
        let mut channel = start(path, false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let handle = value(&completions(&mut channel, 1)[0], 16);
        send(
            &mut channel,
            handle,
            2,
            14,
            &ioctl(0x001b0058, &[0, 0, 0, 0, 17, 19], 0),
        );
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        send(&mut channel, handle, 3, 14, &ioctl(0x001b005c, &[], 6));
        assert_eq!(
            &completions(&mut channel, 1)[0][20..],
            &[0, 0, 0, 0, 17, 19]
        );
        let mut attributes = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), attributes.as_mut_ptr()) },
            0
        );
        let before = unsafe { attributes.assume_init() };
        // PTYs have no modem lines: do not report a fabricated CTS/DTR state.
        send(&mut channel, handle, 4, 14, &ioctl(0x001b0068, &[], 4));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc00000bb);
        let flow: Vec<u8> = [1u32, 0x43, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        send(&mut channel, handle, 5, 14, &ioctl(0x001b0064, &flow, 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc00000bb);
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), attributes.as_mut_ptr()) },
            0
        );
        assert_eq!(unsafe { attributes.assume_init() }.c_iflag, before.c_iflag);
        send(&mut channel, handle, 6, 14, &ioctl(0x001b001c, &[0; 19], 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc000000d);
    }

    #[test]
    fn embedded_serial_read_only_denies_writes_and_mutating_controls() {
        let (mut master, _slave, path) = pty();
        let mut channel = start(path, true);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc0000022);
        send(&mut channel, 0, 2, 0, &create("", 0x80000000, 1, 0));
        let packets = completions(&mut channel, 1);
        assert_eq!(value(&packets[0], 12), 0);
        let handle = value(&packets[0], 16);
        send(&mut channel, handle, 3, 4, &[0; 32]);
        send(
            &mut channel,
            handle,
            4,
            14,
            &ioctl(0x001b0004, &19200u32.to_le_bytes(), 0),
        );
        send(&mut channel, handle, 5, 14, &ioctl(0x12345678, &[], 0));
        let packets = completions(&mut channel, 3);
        assert_eq!(value(&packets[0], 12), 0xc0000022);
        assert_eq!(value(&packets[1], 12), 0xc0000022);
        assert_eq!(value(&packets[2], 12), 0xc00000bb);
        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&5u32.to_le_bytes());
        master.write_all(b"read\n").unwrap();
        send(&mut channel, handle, 6, 3, &transfer);
        let packets = completions(&mut channel, 1);
        assert_eq!(&packets[0][20..], b"read\n");
    }

    #[test]
    fn embedded_drive_and_serial_use_distinct_device_ids_on_the_same_channel() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("sample"), b"data").unwrap();
        let (_master, _slave, path) = pty();
        let mut channel = Channel::with_serial(
            &[DriveMapping {
                name: "Work".into(),
                path: root.path().into(),
                read_only: true,
            }],
            &[SerialMapping {
                name: "COM2".into(),
                path,
                read_only: false,
            }],
        )
        .unwrap();
        channel
            .process(&[0x72, 0x44, 0x6e, 0x49, 1, 0, 12, 0, 1, 0, 0, 0])
            .unwrap();
        channel
            .process(&[0x72, 0x44, 0x43, 0x43, 1, 0, 12, 0, 1, 0, 0, 0])
            .unwrap();
        let announce = channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap()[0]
            .encode_unframed_pdu()
            .unwrap();
        assert_eq!(value(&announce, 4), 2);
        let serial = 28 + value(&announce, 24) as usize;
        assert_eq!(value(&announce, serial), 1);
        assert_eq!(value(&announce, serial + 4), 2);
        assert_eq!(&announce[serial + 8..serial + 16], b"COM2\0\0\0\0");
        send(&mut channel, 0, 1, 0, &create("sample", 0x80000000, 1, 0));
        channel
            .process(&request(2, 0, 2, 0, 0, &create("", 0xc0000000, 1, 0)))
            .unwrap();
        let packets = completions(&mut channel, 2);
        assert_eq!((value(&packets[0], 4), value(&packets[0], 12)), (1, 0));
        assert_eq!((value(&packets[1], 4), value(&packets[1], 12)), (2, 0));
    }

    #[test]
    fn embedded_serial_certificate_retry_keeps_the_original_tty_binding() {
        let (mut original, _original_slave, original_path) = pty();
        let (mut replacement, _replacement_slave, replacement_path) = pty();
        let directory = tempfile::tempdir().unwrap();
        let alias = directory.path().join("selected-serial");
        std::os::unix::fs::symlink(original_path, &alias).unwrap();
        let bindings = BoundSerialMappings::new(&[SerialMapping {
            name: "COM2".into(),
            path: alias.clone(),
            read_only: false,
        }])
        .unwrap();
        let connect = || Channel::with_bound_serial(&[], &bindings).unwrap();
        // The first attempt stops before the static channel handshake when TLS
        // needs a certificate decision.
        drop(connect());

        let next_alias = directory.path().join("replacement-serial");
        std::os::unix::fs::symlink(replacement_path, &next_alias).unwrap();
        std::fs::rename(next_alias, &alias).unwrap();
        let mut retried = handshake(connect());
        send(&mut retried, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let opened = completions(&mut retried, 1);
        assert_eq!(value(&opened[0], 12), 0);
        let mut transfer = vec![0; 32];
        transfer[..4].copy_from_slice(&4u32.to_le_bytes());
        transfer.extend(b"safe");
        send(&mut retried, value(&opened[0], 16), 2, 4, &transfer);
        let written = completions(&mut retried, 1);
        assert_eq!(value(&written[0], 16), 4);
        let mut bytes = [0; 4];
        assert_eq!(
            replacement.read(&mut bytes).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        original.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"safe");
    }

    #[test]
    fn embedded_serial_does_not_reopen_a_recycled_pty() {
        let (master, slave, path) = pty();
        let mut channel = start(path.clone(), false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        let initial = completions(&mut channel, 1);
        assert_eq!(value(&initial[0], 12), 0);
        let handle = value(&initial[0], 16);
        send(&mut channel, handle, 2, 2, &[0; 32]);
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0);
        drop(master);
        drop(slave);

        // devpts can reuse the same path and identity after closing a PTY.
        let mut replacements: Vec<_> = (0..32).map(|_| pty()).collect();
        send(&mut channel, 0, 3, 0, &create("", 0xc0000000, 1, 0));
        let reopened = completions(&mut channel, 1);
        if value(&reopened[0], 12) == 0 {
            let mut transfer = vec![0; 32];
            transfer[..4].copy_from_slice(&4u32.to_le_bytes());
            transfer.extend(b"leak");
            send(&mut channel, value(&reopened[0], 16), 4, 4, &transfer);
            completions(&mut channel, 1);
        }
        for (master, _, _) in &mut replacements {
            assert_eq!(
                master.read(&mut [0; 4]).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
        assert_eq!(value(&reopened[0], 12), 0xc000009d);
    }

    #[test]
    fn embedded_serial_rejects_non_tty_character_devices() {
        let mut channel = start(PathBuf::from("/dev/null"), false);
        send(&mut channel, 0, 1, 0, &create("", 0xc0000000, 1, 0));
        assert_eq!(value(&completions(&mut channel, 1)[0], 12), 0xc00000bb);
    }
}

#[cfg(target_os = "linux")]
#[path = "printer_tests.rs"]
mod printer_tests;
