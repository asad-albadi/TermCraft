use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
fn session() -> Session {
    let mut s = Session {
        protocol: "VNC".into(),
        host: "localhost".into(),
        port: 5900,
        ..Default::default()
    };
    s.vnc_options = VncOptions {
        embedded: true,
        allow_unencrypted: true,
        security: Security::None,
        ..Default::default()
    };
    s
}
fn state() -> Arc<Mutex<State>> {
    Arc::new(Mutex::new(State {
        ready: true,
        cursor: None,
        frame_revision: 0,
        frame: None,
        challenge: None,
        connected: false,
        dimensions: (1, 1),
        final_message: None,
    }))
}
async fn read_encodings(server: &mut (impl AsyncRead + Unpin)) -> Vec<i32> {
    let header = bytes(server, 4).await.unwrap();
    assert_eq!(header[0], 2);
    let count = u16::from_be_bytes(header[2..4].try_into().unwrap()) as usize;
    bytes(server, count * 4)
        .await
        .unwrap()
        .chunks_exact(4)
        .map(|chunk| i32::from_be_bytes(chunk.try_into().unwrap()))
        .collect()
}
async fn init(server: &mut DuplexStream) {
    server.write_all(b"RFB 003.008\n").await.unwrap();
    assert_eq!(bytes(server, 12).await.unwrap(), b"RFB 003.008\n");
    server.write_all(&[1, 1]).await.unwrap();
    assert_eq!(bytes(server, 1).await.unwrap(), [1]);
    server.write_all(&[0; 4]).await.unwrap();
    assert_eq!(bytes(server, 1).await.unwrap(), [1]);
    let mut init = vec![0, 4, 0, 3];
    init.extend([0; 16]);
    init.extend(0u32.to_be_bytes());
    server.write_all(&init).await.unwrap();
    assert_eq!(bytes(server, 20).await.unwrap()[4..8], [32, 24, 0, 1]);
    let enc = read_encodings(server).await;
    assert!(enc.contains(&0));
    assert_eq!(
        bytes(server, 10).await.unwrap(),
        update_request(4, 3, false)
    );
}
async fn update(
    server: &mut DuplexStream,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    encoding: i32,
    data: &[u8],
) {
    let mut p = vec![0, 0, 0, 1];
    for n in [x, y, w, h] {
        p.extend(n.to_be_bytes());
    }
    p.extend(encoding.to_be_bytes());
    p.extend(data);
    // Fragment packets to exercise actual incremental stream reads.
    for part in p.chunks(3) {
        server.write_all(part).await.unwrap();
        tokio::task::yield_now().await;
    }
}
#[test]
fn old_sessions_keep_external_vnc_and_new_security_is_explicit() {
    let s: Session = serde_json::from_str(r#"{"protocol":"VNC","host":"localhost"}"#).unwrap();
    assert!(!s.vnc_options.embedded);
    let mut options = VncOptions {
        embedded: true,
        security: Security::Password,
        ..Default::default()
    };
    assert!(options.validate().is_err());
    options.allow_unencrypted = true;
    assert!(options.validate().is_ok());
    assert!(response("password-too-long", &[0; 16]).is_err());
}
#[tokio::test]
async fn vnc_known_frame_pointer_copyrect_and_resize_are_real_protocol_operations() {
    let (client, mut server) = tokio::io::duplex(4096);
    let shared = state();
    let observed = shared.clone();
    let (queue, rx) = input_channel();
    let (tx, _events) = mpsc::sync_channel(32);
    let fixture = async {
        init(&mut server).await;
        update(&mut server, 1, 1, 1, 1, 0, &[0, 255, 0, 0]).await;
        bytes(&mut server, 10).await.unwrap();
        assert_eq!(
            &observed.lock().unwrap().frame.as_ref().unwrap().pixels[20..24],
            &[0, 255, 0, 255]
        );
        queue
            .try_send(DesktopInput::Pointer {
                x: 2,
                y: 1,
                button: Some(1),
                pressed: true,
            })
            .unwrap();
        assert_eq!(bytes(&mut server, 6).await.unwrap(), [5, 1, 0, 2, 0, 1]);
        update(&mut server, 2, 1, 1, 1, 1, &[0, 1, 0, 1]).await;
        bytes(&mut server, 10).await.unwrap();
        assert_eq!(
            &observed.lock().unwrap().frame.as_ref().unwrap().pixels[24..28],
            &[0, 255, 0, 255]
        );
        update(&mut server, 0, 0, 800, 600, -223, &[]).await;
        assert_eq!(
            bytes(&mut server, 10).await.unwrap(),
            update_request(800, 600, false)
        );
        assert_eq!(observed.lock().unwrap().dimensions, (800, 600));
        queue.try_send(DesktopInput::ReleaseAll).unwrap();
        assert_eq!(bytes(&mut server, 6).await.unwrap(), [5, 0, 0, 2, 0, 1]);
        queue.try_send(DesktopInput::Close).unwrap();
    };
    let s = session();
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(run(client, &s, rx, tx, shared, 2), fixture)
    })
    .await
    .unwrap();
    assert!(result.is_ok(), "{result:?}");
}
#[tokio::test]
async fn view_only_sends_no_input_even_at_transport_boundary_and_clipboard_is_directional() {
    let (client, mut server) = tokio::io::duplex(4096);
    let (queue, rx) = input_channel();
    let (tx, events) = mpsc::sync_channel(32);
    let mut s = session();
    s.view_only = true;
    s.vnc_options.clipboard = rdp_options::ClipboardPolicy::FromRemote;
    let fixture = async {
        init(&mut server).await;
        queue
            .try_send(DesktopInput::Key {
                code: 30,
                extended: false,
                pressed: true,
            })
            .unwrap();
        queue
            .try_send(DesktopInput::Pointer {
                x: 1,
                y: 1,
                button: Some(1),
                pressed: true,
            })
            .unwrap();
        queue
            .try_send(DesktopInput::Clipboard {
                text: "private".into(),
            })
            .unwrap();
        server
            .write_all(&[3, 0, 0, 0, 0, 0, 0, 3, b'a', 0xe9, b'b'])
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(80), server.read_u8())
                .await
                .is_err(),
            "view-only sent input"
        );
        queue.try_send(DesktopInput::ReleaseAll).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), server.read_u8())
                .await
                .is_err()
        );
        let actual = events.try_iter().find_map(|event| {
            if let DesktopEvent::Clipboard { text } = event {
                Some(text)
            } else {
                None
            }
        });
        assert_eq!(actual.as_deref(), Some("aéb"));
        queue.try_send(DesktopInput::Close).unwrap();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(run(client, &s, rx, tx, state(), 2), fixture)
    })
    .await
    .unwrap();
    assert!(result.is_ok(), "{result:?}");
}
#[tokio::test]
async fn oversized_resize_and_disconnect_mid_rectangle_are_errors() {
    for oversize in [true, false] {
        let (client, mut server) = tokio::io::duplex(4096);
        let (_queue, rx) = input_channel();
        let (tx, _events) = mpsc::sync_channel(32);
        let fixture = async {
            init(&mut server).await;
            if oversize {
                update(&mut server, 0, 0, 65535, 65535, -223, &[]).await;
            } else {
                update(&mut server, 0, 0, 1, 1, 0, &[0]).await;
            }
            drop(server);
        };
        let s = session();
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(run(client, &s, rx, tx, state(), 2), fixture)
        })
        .await
        .unwrap();
        assert!(result.is_err());
    }
}

#[test]
fn classic_vnc_challenge_matches_independent_openssl_vector() {
    // openssl enc -des-ecb -provider legacy -K 0e86ceceeef64e26 -nopad,
    // input bytes 00..0f. Key is the bit-reversal of ASCII "password".
    assert_eq!(
        response("password", &(0u8..16).collect::<Vec<_>>())
            .unwrap()
            .as_slice(),
        &[
            0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb, 0x61, 0xc5,
            0x38, 0xe2
        ]
    );
}
#[tokio::test]
async fn selected_security_never_falls_back_to_another_server_offer() {
    let (client, mut server) = tokio::io::duplex(4096);
    let (queue, rx) = input_channel();
    let (tx, _events) = mpsc::sync_channel(32);
    let mut s = session();
    s.vnc_options.security = Security::Password;
    let fixture = async {
        server.write_all(b"RFB 003.008\n").await.unwrap();
        bytes(&mut server, 12).await.unwrap();
        server.write_all(&[1, 1]).await.unwrap();
        assert_eq!(
            server.read(&mut [0u8; 1]).await.unwrap(),
            0,
            "client sent authentication to a different security type"
        );
        drop(queue);
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(run(client, &s, rx, tx, state(), 2), fixture)
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("selected security type"));
}

#[tokio::test]
async fn classic_password_wait_does_not_spend_network_timeout_or_leak_plaintext() {
    let (client, mut server) = tokio::io::duplex(4096);
    let shared = state();
    let observed = shared.clone();
    let (queue, rx) = input_channel();
    let (tx, _events) = mpsc::sync_channel(32);
    let mut s = session();
    s.vnc_options.security = Security::Password;
    let fixture = async {
        server.write_all(b"RFB 003.008\n").await.unwrap();
        bytes(&mut server, 12).await.unwrap();
        server.write_all(&[1, 2]).await.unwrap();
        assert_eq!(bytes(&mut server, 1).await.unwrap(), [2]);
        server
            .write_all(&(0u8..16).collect::<Vec<_>>())
            .await
            .unwrap();
        while observed.lock().unwrap().challenge.is_none() {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(1100)).await;
        queue
            .try_send(DesktopInput::Authenticate {
                id: 1,
                user: "VNC".into(),
                password: crate::credentials::Secret::new("password"),
                domain: String::new(),
            })
            .unwrap();
        assert_eq!(
            bytes(&mut server, 16).await.unwrap(),
            [
                0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb, 0x61, 0xc5,
                0x38, 0xe2
            ]
        );
        server.write_all(&1u32.to_be_bytes()).await.unwrap();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(run(client, &s, rx, tx, shared, 1), fixture)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err(), "VNC authentication was rejected");
}

#[test]
fn verified_tls_modes_do_not_require_classic_unencrypted_consent() {
    for security in [Security::TlsPassword, Security::TlsNone] {
        let options = VncOptions {
            embedded: true,
            security,
            ..Default::default()
        };
        assert!(options.validate().is_ok());
    }
}

struct TlsFixture {
    root: tempfile::TempDir,
    config: Arc<rustls::ServerConfig>,
}
impl TlsFixture {
    fn new() -> Self {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
        let root = tempfile::tempdir().unwrap();
        let cert = root.path().join("cert.pem");
        let ca_key = root.path().join("ca.key");
        let csr = root.path().join("server.csr");
        let serial = root.path().join("ca.srl");
        let server_cert = root.path().join("server.pem");
        let key = root.path().join("key.pem");
        let ca = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=TermCraft test CA",
                "-addext",
                "basicConstraints=critical,CA:TRUE,pathlen:1",
                "-keyout",
            ])
            .arg(&ca_key)
            .arg("-out")
            .arg(&cert)
            .output()
            .unwrap();
        assert!(
            ca.status.success(),
            "CA fixture failed: {}",
            String::from_utf8_lossy(&ca.stderr)
        );
        let request = std::process::Command::new("openssl")
            .args([
                "req",
                "-new",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-subj",
                "/CN=localhost",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&csr)
            .output()
            .unwrap();
        assert!(
            request.status.success(),
            "certificate request fixture failed: {}",
            String::from_utf8_lossy(&request.stderr)
        );
        let extensions = root.path().join("server.ext");
        std::fs::write(
            &extensions,
            "subjectAltName=DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
        )
        .unwrap();
        let signed = std::process::Command::new("openssl")
            .args(["x509", "-req", "-days", "1", "-CAcreateserial", "-CA"])
            .arg(&cert)
            .arg("-CAkey")
            .arg(&ca_key)
            .arg("-CAserial")
            .arg(&serial)
            .arg("-in")
            .arg(&csr)
            .arg("-out")
            .arg(&server_cert)
            .arg("-extfile")
            .arg(&extensions)
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "signed certificate fixture failed: {}",
            String::from_utf8_lossy(&signed.stderr)
        );
        let certificate = CertificateDer::from_pem_file(&server_cert).unwrap();
        let key = PrivateKeyDer::from_pem_file(&key).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap();
        Self {
            root,
            config: Arc::new(config),
        }
    }
}
async fn vencrypt_server(
    mut server: DuplexStream,
    config: Arc<rustls::ServerConfig>,
    password: bool,
) -> Result<tokio_rustls::server::TlsStream<DuplexStream>, std::io::Error> {
    server.write_all(b"RFB 003.008\n").await.unwrap();
    assert_eq!(bytes(&mut server, 12).await.unwrap(), b"RFB 003.008\n");
    // Offer insecure None alongside VeNCrypt. The TLS client must never pick it.
    server.write_all(&[2, 1, 19]).await.unwrap();
    assert_eq!(bytes(&mut server, 1).await.unwrap(), [19]);
    server.write_all(&[0, 2]).await.unwrap();
    assert_eq!(bytes(&mut server, 2).await.unwrap(), [0, 2]);
    server.write_all(&[0, 1]).await.unwrap();
    let subtype = if password { 261u32 } else { 260 };
    server.write_all(&subtype.to_be_bytes()).await.unwrap();
    assert_eq!(number(&mut server).await.unwrap(), subtype);
    server.write_all(&[1]).await.unwrap();
    tokio_rustls::TlsAcceptor::from(config).accept(server).await
}
#[tokio::test]
async fn vencrypt_verified_none_and_password_deliver_pixels_over_real_tls() {
    let tls = TlsFixture::new();
    for password in [false, true] {
        let (client, server) = tokio::io::duplex(65536);
        let (queue, rx) = input_channel();
        let (tx, events) = mpsc::sync_channel(32);
        let shared = state();
        let observed = shared.clone();
        let mut s = session();
        s.vnc_options.security = if password {
            Security::TlsPassword
        } else {
            Security::TlsNone
        };
        s.vnc_options.allow_unencrypted = false;
        s.vnc_options.certificate_authority = Some(tls.root.path().join("cert.pem"));
        let fixture = async {
            let mut server = vencrypt_server(server, tls.config.clone(), password)
                .await
                .unwrap();
            if password {
                server
                    .write_all(&(0u8..16).collect::<Vec<_>>())
                    .await
                    .unwrap();
                loop {
                    match events.try_recv() {
                        Ok(DesktopEvent::Authentication { fingerprint, .. }) => {
                            assert_eq!(fingerprint.len(), 64);
                            queue
                                .try_send(DesktopInput::Authenticate {
                                    id: 1,
                                    user: "VNC".into(),
                                    password: crate::credentials::Secret::new("password"),
                                    domain: String::new(),
                                })
                                .unwrap();
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                        _ => panic!("expected verified TLS authentication challenge"),
                    }
                }
                assert_eq!(
                    bytes(&mut server, 16).await.unwrap(),
                    [
                        0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb,
                        0x61, 0xc5, 0x38, 0xe2
                    ]
                );
            }
            server.write_all(&[0; 4]).await.unwrap();
            assert_eq!(bytes(&mut server, 1).await.unwrap(), [1]);
            let mut init = vec![0, 4, 0, 3];
            init.extend([0; 16]);
            init.extend(0u32.to_be_bytes());
            server.write_all(&init).await.unwrap();
            bytes(&mut server, 20).await.unwrap();
            read_encodings(&mut server).await;
            assert_eq!(
                bytes(&mut server, 10).await.unwrap(),
                update_request(4, 3, false)
            );
            let mut frame = vec![0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
            frame.extend(0i32.to_be_bytes());
            frame.extend([0, 255, 0, 0]);
            server.write_all(&frame).await.unwrap();
            bytes(&mut server, 10).await.unwrap();
            assert_eq!(
                &observed.lock().unwrap().frame.as_ref().unwrap().pixels[20..24],
                &[0, 255, 0, 255]
            );
            queue.try_send(DesktopInput::Close).unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(run(client, &s, rx, tx, shared, 3), fixture)
        })
        .await
        .unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}
#[tokio::test]
async fn vencrypt_bad_trust_hostname_and_rotated_certificate_prevent_authentication() {
    let original = TlsFixture::new();
    let rotated = TlsFixture::new();
    for failure in ["untrusted", "hostname", "rotated"] {
        let (client, server) = tokio::io::duplex(65536);
        let (_queue, rx) = input_channel();
        let (tx, events) = mpsc::sync_channel(32);
        let mut s = session();
        s.vnc_options.security = Security::TlsPassword;
        s.vnc_options.allow_unencrypted = false;
        if failure != "untrusted" {
            s.vnc_options.certificate_authority = Some(original.root.path().join("cert.pem"));
        }
        if failure == "hostname" {
            s.host = "other.example".into();
        }
        let config = if failure == "rotated" {
            rotated.config.clone()
        } else {
            original.config.clone()
        };
        let fixture = async {
            assert!(
                vencrypt_server(server, config, true).await.is_err(),
                "server accepted invalid client trust"
            );
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(run(client, &s, rx, tx, state(), 3), fixture)
        })
        .await
        .unwrap();
        assert!(
            result
                .unwrap_err()
                .contains("certificate/hostname validation"),
            "{failure}"
        );
        assert!(
            !events.try_iter().any(|event| matches!(
                event,
                DesktopEvent::Authentication { .. } | DesktopEvent::Connected { .. }
            )),
            "authentication was exposed before TLS verification: {failure}"
        );
    }
}

#[tokio::test]
async fn resize_waits_for_advertisement_and_preserves_screen_identity() {
    let (client, mut server) = tokio::io::duplex(4096);
    let (queue, rx) = input_channel();
    let (tx, _events) = mpsc::sync_channel(32);
    let observed = state();
    let fixture = async {
        init(&mut server).await;
        queue
            .try_send(DesktopInput::Resize {
                width: 8,
                height: 6,
            })
            .unwrap();
        // A server limited to standard RFB must never receive type 251.
        let pointer = DesktopInput::Pointer {
            x: 1,
            y: 2,
            button: None,
            pressed: false,
        };
        queue.try_send(pointer).unwrap();
        assert_eq!(bytes(&mut server, 6).await.unwrap(), [5, 0, 0, 1, 0, 2]);

        let initial = [
            1, 0, 0, 0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 4, 0, 3, 0x80, 0, 0, 1,
        ];
        update(&mut server, 0, 0, 4, 3, -308, &initial).await;
        assert_eq!(
            bytes(&mut server, 10).await.unwrap(),
            update_request(4, 3, true)
        );
        let request = [
            251, 0, 0, 8, 0, 6, 1, 0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 8, 0, 6, 0x80, 0, 0, 1,
        ];
        assert_eq!(bytes(&mut server, request.len()).await.unwrap(), request);
        assert_eq!(observed.lock().unwrap().dimensions, (4, 3));

        for width in [9, 10, 0] {
            queue
                .try_send(DesktopInput::Resize { width, height: 7 })
                .unwrap();
        }
        queue
            .try_send(DesktopInput::Pointer {
                x: 2,
                y: 2,
                button: None,
                pressed: false,
            })
            .unwrap();
        // The in-flight request blocks additional resize writes, without blocking input.
        assert_eq!(bytes(&mut server, 6).await.unwrap(), [5, 0, 0, 2, 0, 2]);
        // Decline fields beyond status are undefined and must not change the frame.
        update(&mut server, 1, 3, 0, 0, -308, &[0; 4]).await;
        assert_eq!(
            bytes(&mut server, 10).await.unwrap(),
            update_request(4, 3, true)
        );
        assert_eq!(observed.lock().unwrap().dimensions, (4, 3));

        let packet = bytes(&mut server, request.len()).await.unwrap();
        assert_eq!(&packet[..8], &[251, 0, 0, 10, 0, 7, 1, 0]);
        let mut accepted = initial;
        accepted[13] = 10;
        accepted[15] = 7;
        update(&mut server, 1, 0, 10, 7, -308, &accepted).await;
        assert_eq!(
            bytes(&mut server, 10).await.unwrap(),
            update_request(10, 7, true)
        );
        assert_eq!(observed.lock().unwrap().dimensions, (10, 7));
        // A resized RFB framebuffer is invalidated server-side and repainted even
        // though the client request must be incremental after ExtendedDesktopSize.
        update(&mut server, 0, 0, 10, 7, 0, &[0, 255, 0, 0].repeat(70)).await;
        assert_eq!(
            bytes(&mut server, 10).await.unwrap(),
            update_request(10, 7, true)
        );
        let frame = observed.lock().unwrap().frame.clone().unwrap();
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
        queue.try_send(DesktopInput::Close).unwrap();
    };
    let session = session();
    let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(run(client, &session, rx, tx, observed.clone(), 2), fixture)
    })
    .await
    .unwrap();
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn input_wakeups_are_retained_through_empty_queue_and_drop() {
    use futures::FutureExt;
    let (queue, rx) = input_channel();
    assert!(rx.control.ready.notified().now_or_never().is_none());
    queue.try_send(DesktopInput::Close).unwrap();
    assert!(rx.control.ready.notified().now_or_never().is_some());
    assert!(matches!(
        rx.rx.try_recv().unwrap().input,
        DesktopInput::Close
    ));
    assert!(matches!(rx.rx.try_recv(), Err(mpsc::TryRecvError::Empty)));

    queue.try_send(DesktopInput::ReleaseAll).unwrap();
    assert!(rx.control.ready.notified().now_or_never().is_some());
    assert!(rx.control.release.load(Ordering::Acquire));
    assert!(matches!(rx.rx.try_recv(), Err(mpsc::TryRecvError::Empty)));

    drop(queue);
    assert!(rx.control.ready.notified().now_or_never().is_some());
    assert!(matches!(
        rx.rx.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}
