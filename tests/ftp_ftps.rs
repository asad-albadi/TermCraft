#[path = "support/ftp_server.rs"]
mod fixture;

use fixture::{Endpoint, ServerOptions};
use harbor_workspace::{
    credentials::Secret,
    protocols::ftp::{FtpBackend, FtpDataMode, FtpTlsMode, FtpTransferType},
    remote_files::{EntryKind, RemotePath},
};
use std::time::Duration;

fn path(bytes: &[u8]) -> RemotePath {
    RemotePath::from_bytes(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn binary_transfer_is_byte_exact() {
    for data_mode in [FtpDataMode::Passive, FtpDataMode::Active] {
        let server = Endpoint::start(ServerOptions::default()).await;
        let mut options = server.options();
        options.data_mode = data_mode;
        let mut ftp = FtpBackend::connect(options).await.unwrap();
        assert!(ftp.certificate_fingerprint().is_none());
        let file = path(b"/a file");
        let binary_upload = b"\0\xffone\rtwo\nthree\r\n\0";
        ftp.write(&file, FtpTransferType::Binary, binary_upload)
            .await
            .unwrap();
        let binary_download = ftp
            .read(&file, FtpTransferType::Binary, 1024)
            .await
            .unwrap();
        assert_eq!(binary_download, binary_upload);
        let entries = ftp.list(&path(b"/")).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, file);
        assert_eq!(entries[0].size, Some(binary_upload.len() as u64));
        assert_eq!(ftp.size(&file).await.unwrap(), binary_upload.len() as u64);
        let renamed = path(b"/renamed");
        ftp.rename(&file, &renamed).await.unwrap();
        ftp.remove(&renamed, EntryKind::File).await.unwrap();
        let directory = path(b"/empty");
        ftp.mkdir(&directory).await.unwrap();
        ftp.remove(&directory, EntryKind::Directory).await.unwrap();
        let evidence = server.evidence();
        assert_eq!(evidence.uploads, [binary_upload.to_vec()]);
        assert_eq!(evidence.data_connections, 3);
        assert_eq!(
            evidence.active_connections,
            if data_mode == FtpDataMode::Active {
                3
            } else {
                0
            }
        );
    }
}

#[tokio::test]
async fn ascii_mode_converts_only_selected_text() {
    let server = Endpoint::start(ServerOptions::default()).await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    let text = path(b"/text");
    ftp.write(&text, FtpTransferType::Ascii, b"one\ntwo\r\n")
        .await
        .unwrap();
    let converted_text = &server.evidence().uploads[0];
    assert_eq!(converted_text, b"one\r\ntwo\r\n");
    assert_eq!(
        ftp.read(&text, FtpTransferType::Ascii, 1024).await.unwrap(),
        b"one\ntwo\n"
    );
    assert_eq!(
        ftp.read(&text, FtpTransferType::Binary, 1024)
            .await
            .unwrap(),
        b"one\r\ntwo\r\n"
    );
}

#[tokio::test]
async fn ftps_validates_certificate_and_encrypts_selected_channels() {
    for tls in [FtpTlsMode::Explicit, FtpTlsMode::Implicit] {
        for data_mode in [FtpDataMode::Passive, FtpDataMode::Active] {
            let server = Endpoint::start(ServerOptions {
                tls,
                ..Default::default()
            })
            .await;
            let mut options = server.options();
            options.data_mode = data_mode;
            let mut ftp = FtpBackend::connect(options).await.unwrap();
            assert_eq!(ftp.certificate_fingerprint().unwrap().len(), 64);
            let file = path(b"/tls");
            ftp.write(&file, FtpTransferType::Binary, b"protected\0")
                .await
                .unwrap();
            assert_eq!(
                ftp.read(&file, FtpTransferType::Binary, 256).await.unwrap(),
                b"protected\0"
            );
            let evidence = server.evidence();
            assert_eq!(evidence.encrypted_control_logins, 1);
            assert_eq!(evidence.encrypted_data_connections, 2);
        }
        for wrong_name in [true, false] {
            let server = Endpoint::start(ServerOptions {
                tls,
                ..Default::default()
            })
            .await;
            let mut options = server.options();
            if wrong_name {
                options.host = "127.0.0.1".into();
            } else {
                options.certificate_authority = None;
            }
            let invalid_certificate_connected = FtpBackend::connect(options).await.is_ok();
            assert!(!invalid_certificate_connected);
            assert_eq!(server.evidence().login_attempts, 0);
        }
    }
}

#[tokio::test]
async fn rejected_protection_and_plain_data_do_not_downgrade() {
    let tls = FtpTlsMode::Explicit;
    let server = Endpoint::start(ServerOptions {
        tls,
        reject_protection: true,
        ..Default::default()
    })
    .await;
    let failure = FtpBackend::connect(server.options()).await.err().unwrap();
    assert!(failure.contains("PROT P"), "{failure}");
    assert_eq!(server.evidence().clear_protection_requests, 0);
    let server = Endpoint::start(ServerOptions {
        tls,
        force_clear_data: true,
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .is_err()
    );
    assert_eq!(server.evidence().data_connections, 1);
    assert_eq!(server.evidence().clear_protection_requests, 0);
}

#[tokio::test]
async fn verifies_active_peer_and_ignores_pasv_address() {
    let server = Endpoint::start(ServerOptions {
        force_pasv: true,
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert_eq!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .unwrap(),
        b"seed bytes"
    );
    let server = Endpoint::start(ServerOptions {
        force_port: true,
        foreign_active_peer: true,
        ..Default::default()
    })
    .await;
    let mut options = server.options();
    options.data_mode = FtpDataMode::Active;
    let mut ftp = FtpBackend::connect(options).await.unwrap();
    assert_eq!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .unwrap(),
        b"seed bytes"
    );
    assert_eq!(server.evidence().foreign_active_connections, 1);
}

#[tokio::test]
async fn cancel_interrupts_stalled_transfer_and_paths_cannot_inject_commands() {
    let server = Endpoint::start(ServerOptions {
        stall_transfer: true,
        ..Default::default()
    })
    .await;
    let mut options = server.options();
    options.timeout = Duration::from_secs(10);
    let mut ftp = FtpBackend::connect(options).await.unwrap();
    assert!(ftp.mkdir(&path(b"a\r\nDELE /seed")).await.is_err());
    assert!(
        !server
            .evidence()
            .commands
            .iter()
            .any(|command| command == "MKD" || command == "DELE")
    );
    let cancel = ftp.cancel_handle();
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let signal = async {
            while server.evidence().data_connections == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            cancel.cancel();
        };
        let seed = path(b"/seed");
        tokio::join!(ftp.read(&seed, FtpTransferType::Binary, 128), signal).0
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("cancel"));
}

#[tokio::test]
async fn invalid_credentials_never_open_control_channel() {
    let server = Endpoint::start(ServerOptions::default()).await;
    let mut options = server.options();
    options.password = Some(Secret::new("password\r\nDELE /seed"));
    assert!(FtpBackend::connect(options).await.is_err());
    assert!(server.evidence().commands.is_empty());
}

#[tokio::test]
async fn bounded_downloads_require_final_success_and_idle_transfers_time_out() {
    let server = Endpoint::start(ServerOptions::default()).await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 2)
            .await
            .unwrap_err()
            .contains("permitted size")
    );
    assert!(!ftp.is_connected());
    let server = Endpoint::start(ServerOptions {
        refuse_completion: true,
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .unwrap_err()
            .contains("426")
    );
    let server = Endpoint::start(ServerOptions {
        stall_transfer: true,
        ..Default::default()
    })
    .await;
    let mut options = server.options();
    options.timeout = Duration::from_millis(150);
    let mut ftp = FtpBackend::connect(options).await.unwrap();
    assert!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .unwrap_err()
            .contains("timed out")
    );
}

#[tokio::test]
async fn machine_listing_retains_byte_names_and_rejects_unusable_snapshots() {
    let listing = b"type=cdir; .\r\ntype=pdir; ..\r\ntype=dir; a folder\r\ntype=file;size=2; \xff\r\ntype=OS.unix=slink; shortcut\r\n bare name\r\n";
    let server = Endpoint::start(ServerOptions {
        listing: Some(listing.to_vec()),
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    let entries = ftp.list(&path(b"/dir")).await.unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].kind, EntryKind::Directory);
    assert_eq!(entries[1].path.as_bytes(), b"/dir/\xff");
    assert_eq!(entries[2].kind, EntryKind::Symlink);
    assert_eq!(entries[3].kind, EntryKind::Other);
    assert_eq!(entries[3].path.as_bytes(), b"/dir/bare name");
    let listing = (0..10_001)
        .flat_map(|id| format!("type=file; {id}\r\n").into_bytes())
        .collect();
    let server = Endpoint::start(ServerOptions {
        listing: Some(listing),
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(
        ftp.list(&path(b"/"))
            .await
            .unwrap_err()
            .contains("10,000-entry")
    );
    let listing = b"type=file; ../outside\r\n";
    let server = Endpoint::start(ServerOptions {
        listing: Some(listing.to_vec()),
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(ftp.list(&path(b"/")).await.is_err());
}

#[tokio::test]
async fn protected_download_acknowledges_data_close_before_control_completion() {
    let server = Endpoint::start(ServerOptions {
        tls: FtpTlsMode::Explicit,
        wait_for_data_close: true,
        ..Default::default()
    })
    .await;
    let mut options = server.options();
    options.timeout = Duration::from_secs(1);
    let mut ftp = FtpBackend::connect(options).await.unwrap();
    assert_eq!(
        ftp.read(&path(b"/seed"), FtpTransferType::Binary, 128)
            .await
            .unwrap(),
        b"seed bytes"
    );
    assert_eq!(server.evidence().reciprocal_data_closes, 1);
}

#[tokio::test]
async fn unexpected_preliminary_reply_cannot_supply_next_command_result() {
    let server = Endpoint::start(ServerOptions {
        misleading_pwd_reply: true,
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    assert!(ftp.pwd().await.is_err());
    assert!(ftp.mkdir(&path(b"/refused")).await.is_err());
    assert!(!ftp.is_connected());
    assert!(
        !server
            .evidence()
            .commands
            .iter()
            .any(|command| command == "MKD")
    );
}

#[tokio::test]
async fn pwd_preserves_raw_telnet_escaped_path_round_trip() {
    let server = Endpoint::start(ServerOptions {
        raw_pwd: true,
        ..Default::default()
    })
    .await;
    let mut ftp = FtpBackend::connect(server.options()).await.unwrap();
    let current = ftp.pwd().await.unwrap();
    assert_eq!(current.as_bytes(), b"/\xff");
    ftp.mkdir(&current).await.unwrap();
    assert_eq!(
        server.evidence().byte_path_commands,
        [b"MKD /\xff\xff\r\n".to_vec()]
    );
}
