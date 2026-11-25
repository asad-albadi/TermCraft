//! New V03 endpoint observation: verified VeNCrypt over real private TCP,
//! using DesktopSession rather than an in-memory protocol worker stream.
#[allow(dead_code)]
#[path = "support/vnc_server.rs"]
mod fixture;
use harbor_workspace::desktop::{
    DesktopEvent, DesktopInput, DesktopSession, rdp_options::ClipboardPolicy, vnc::Security,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn certificate(root: &std::path::Path) -> Arc<rustls::ServerConfig> {
    let key = root.join("key.pem");
    let cert = root.join("cert.pem");
    let output = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .output()
        .unwrap();
    assert!(output.status.success());
    Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from_pem_file(cert).unwrap()],
            PrivateKeyDer::from_pem_file(key).unwrap(),
        )
        .unwrap(),
    )
}
async fn bytes(s: &mut (impl tokio::io::AsyncRead + Unpin), n: usize) -> Vec<u8> {
    let mut b = vec![0; n];
    s.read_exact(&mut b).await.unwrap();
    b
}
async fn tls(
    listener: TcpListener,
    config: Arc<rustls::ServerConfig>,
) -> Result<tokio_rustls::server::TlsStream<tokio::net::TcpStream>, std::io::Error> {
    listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let (mut s, peer) = listener.accept().await.unwrap();
    assert!(peer.ip().is_loopback());
    s.write_all(b"RFB 003.008\n").await.unwrap();
    assert_eq!(bytes(&mut s, 12).await, b"RFB 003.008\n");
    s.write_all(&[2, 1, 19]).await.unwrap();
    assert_eq!(bytes(&mut s, 1).await, [19]);
    s.write_all(&[0, 2]).await.unwrap();
    assert_eq!(bytes(&mut s, 2).await, [0, 2]);
    s.write_all(&[0, 1]).await.unwrap();
    s.write_all(&261u32.to_be_bytes()).await.unwrap();
    assert_eq!(bytes(&mut s, 4).await, 261u32.to_be_bytes());
    s.write_all(&[1]).await.unwrap();
    tokio_rustls::TlsAcceptor::from(config).accept(s).await
}
#[test]
fn vnc_tls_password_real_tcp_pixels_input_and_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let tls_config = certificate(root.path());
    let (listener, mut config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    config.stream = Some(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
    config.session.host = "localhost".into();
    config.session.vnc_options.security = Security::TlsPassword;
    config.session.vnc_options.allow_unencrypted = false;
    config.session.vnc_options.certificate_authority = Some(root.path().join("cert.pem"));
    let (input_seen, input_observed) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut s = tls(listener, tls_config).await.unwrap();
            s.write_all(&(0u8..16).collect::<Vec<_>>()).await.unwrap();
            assert_eq!(
                bytes(&mut s, 16).await,
                [
                    0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb, 0x61,
                    0xc5, 0x38, 0xe2
                ]
            );
            s.write_all(&[0; 4]).await.unwrap();
            assert_eq!(bytes(&mut s, 1).await, [1]);
            let mut init = vec![0, 4, 0, 3];
            init.extend([0; 16]);
            init.extend(0u32.to_be_bytes());
            s.write_all(&init).await.unwrap();
            bytes(&mut s, 20).await;
            let enc = bytes(&mut s, 4).await;
            bytes(&mut s, u16::from_be_bytes([enc[2], enc[3]]) as usize * 4).await;
            bytes(&mut s, 10).await;
            let mut frame = vec![0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
            frame.extend(0i32.to_be_bytes());
            frame.extend([0, 255, 0, 0]);
            s.write_all(&frame).await.unwrap();
            bytes(&mut s, 10).await;
            assert_eq!(bytes(&mut s, 6).await, [5, 1, 0, 2, 0, 1]);
            input_seen.send(()).unwrap();
            let mut b = [0; 1];
            match s.read(&mut b).await {
                Ok(0) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                other => panic!("carrier not closed: {other:?}"),
            };
        })
    });
    let mut desktop = DesktopSession::connect(config).unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    let mut authenticated = false;
    'frame: loop {
        for event in desktop.poll() {
            match event {
                DesktopEvent::Authentication {
                    id, fingerprint, ..
                } => {
                    assert_eq!(fingerprint.len(), 64);
                    authenticated = true;
                    desktop
                        .send(DesktopInput::Authenticate {
                            id,
                            user: "VNC".into(),
                            password: harbor_workspace::credentials::Secret::new("password"),
                            domain: String::new(),
                        })
                        .unwrap();
                }
                DesktopEvent::Frame(f) => {
                    assert_eq!((f.width, f.height), (4, 3));
                    if f.pixels[20..24] == [0, 255, 0, 255] {
                        break 'frame;
                    }
                }
                DesktopEvent::Disconnected { message } => panic!("{message}"),
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(authenticated);
    desktop
        .send(DesktopInput::Pointer {
            x: 2,
            y: 1,
            button: Some(1),
            pressed: true,
        })
        .unwrap();
    input_observed.recv_timeout(Duration::from_secs(5)).unwrap();
    desktop.close();
    server.join().unwrap();
    println!(
        "Verified TLS password, exact green pixel and pointer bytes over private loopback TCP; owned carrier closed."
    );
}
#[test]
fn vnc_untrusted_tls_real_tcp_never_prompts_for_password() {
    let root = tempfile::tempdir().unwrap();
    let tls_config = certificate(root.path());
    let other = tempfile::tempdir().unwrap();
    let _ = certificate(other.path());
    let (listener, mut config) = fixture::endpoint(false, ClipboardPolicy::Disabled);
    config.stream = Some(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
    config.session.host = "localhost".into();
    config.session.vnc_options.security = Security::TlsPassword;
    config.session.vnc_options.allow_unencrypted = false;
    config.session.vnc_options.certificate_authority = Some(other.path().join("cert.pem"));
    let server = std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(tls(listener, tls_config).await.is_err());
        })
    });
    let mut desktop = DesktopSession::connect(config).unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    'done: loop {
        for event in desktop.poll() {
            match event {
                DesktopEvent::Authentication { .. } => {
                    panic!("Password prompt before certificate trust")
                }
                DesktopEvent::Disconnected { message } => {
                    assert!(message.to_lowercase().contains("certificate"), "{message}");
                    break 'done;
                }
                _ => {}
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
    desktop.close();
    server.join().unwrap();
    println!(
        "Untrusted loopback TLS certificate rejected before authentication prompt or challenge response."
    );
}
