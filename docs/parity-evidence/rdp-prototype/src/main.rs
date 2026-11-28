//! Real loopback engine evaluation, separate from the product.
mod clipboard;
#[path = "../../../../engines/rdp/src/clipboard.rs"]
mod clipboard_backend;
mod config;
#[path = "../../../../tests/support/tls.rs"]
#[allow(dead_code)]
mod tls_fixture;
use ironrdp::cliprdr as clipapi;
use ironrdp::core as clipcore;
use ironrdp::{
    graphics::image_processing::PixelFormat as ImageFormat,
    pdu::input::{
        fast_path::{FastPathInputEvent, KeyboardFlags},
        mouse::{MousePdu, PointerFlags},
    },
    server::{
        self, BitmapUpdate, DisplayUpdate, KeyboardEvent, MouseEvent, PixelFormat,
        RdpServerDisplay, RdpServerDisplayUpdates, RdpServerInputHandler, TlsIdentityCtx,
    },
    session::{ActiveStageBuilder, ActiveStageOutput, image::DecodedImage},
};
use ironrdp_tokio::{FramedWrite, NetworkClient, TokioFramed};
use std::{
    num::{NonZeroU16, NonZeroUsize},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default)]
struct Handler(
    Arc<Mutex<Vec<String>>>,
    Option<std::path::PathBuf>,
    bool,
    Option<Arc<Mutex<server::DesktopSize>>>,
    Option<server::DesktopSize>,
    Option<Arc<Mutex<Option<server::DesktopSize>>>>,
);
impl Handler {
    fn record(&self, event: String) {
        self.0.lock().unwrap().push(event.clone());
        if let Some(path) = &self.1 {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{event}");
            }
        }
    }
}
impl RdpServerInputHandler for Handler {
    fn keyboard(&mut self, event: KeyboardEvent) {
        self.record(format!("key:{event:?}"));
    }
    fn mouse(&mut self, event: MouseEvent) {
        self.record(format!("mouse:{event:?}"));
    }
}
struct Updates {
    requested: Option<Arc<Mutex<Option<server::DesktopSize>>>>,
    cursor: bool,
    phase: u8,
    resize: Option<server::DesktopSize>,
    resize_gate: Option<std::path::PathBuf>,
    dimensions: Option<Arc<Mutex<server::DesktopSize>>>,
}
#[async_trait::async_trait]
impl RdpServerDisplayUpdates for Updates {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        tokio::time::sleep(Duration::from_millis(30)).await;
        self.phase = self.phase.saturating_add(1);
        let request = self
            .requested
            .as_ref()
            .and_then(|slot| slot.lock().unwrap().take());
        if let (Some(target), Some(dimensions)) = (request, &self.dimensions) {
            *dimensions.lock().unwrap() = target;
            return Ok(Some(DisplayUpdate::Resize(target)));
        }
        if let Some(target) = self.resize {
            if self.phase >= 4 && self.resize_gate.as_ref().is_none_or(|path| path.is_file()) {
                if let Some(dimensions) = &self.dimensions {
                    let mut size = dimensions.lock().unwrap();
                    if size.width != target.width || size.height != target.height {
                        *size = target;
                        return Ok(Some(DisplayUpdate::Resize(*size)));
                    }
                }
            }
        }
        if self.cursor {
            if self.phase == 2 {
                return Ok(Some(DisplayUpdate::PointerPosition(
                    ironrdp::pdu::pointer::PointerPositionAttribute { x: 20, y: 20 },
                )));
            }
            if self.phase == 3 {
                return Ok(Some(DisplayUpdate::RGBAPointer(server::RGBAPointer {
                    cache_index: 1,
                    width: 2,
                    height: 2,
                    hot_x: 0,
                    hot_y: 0,
                    data: vec![0, 255, 0, 255].repeat(4),
                })));
            }
        }
        Ok(Some(DisplayUpdate::Bitmap(BitmapUpdate {
            x: 0,
            y: 0,
            width: NonZeroU16::new(64).unwrap(),
            height: NonZeroU16::new(64).unwrap(),
            format: PixelFormat::BgrA32,
            data: vec![0, 0, 255, 255].repeat(64 * 64).into(),
            stride: NonZeroUsize::new(64 * 4).unwrap(),
        })))
    }
}
#[async_trait::async_trait]
impl RdpServerDisplay for Handler {
    async fn size(&mut self) -> server::DesktopSize {
        if let Some(size) = &self.3 {
            return *size.lock().unwrap();
        }
        server::DesktopSize {
            width: 640,
            height: 480,
        }
    }
    fn request_layout(&mut self, layout: ironrdp_displaycontrol::pdu::DisplayControlMonitorLayout) {
        if let [monitor] = layout.monitors() {
            let (width, height) = monitor.dimensions();
            if monitor.is_primary()
                && (200..=8192).contains(&width)
                && (200..=8192).contains(&height)
            {
                self.record(format!("client_resize:{width}x{height}"));
                if let Some(requested) = &self.5 {
                    *requested.lock().unwrap() = Some(server::DesktopSize {
                        width: width as u16,
                        height: height as u16,
                    });
                }
            }
        }
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        Ok(Box::new(Updates {
            requested: self.5.clone(),
            cursor: self.2,
            phase: 0,
            resize: self.4,
            resize_gate: if self.4.is_some_and(|size| size.width == 640) {
                self.1
                    .as_ref()
                    .map(|path| path.with_file_name("resize-now"))
            } else {
                None
            },
            dimensions: self.3.clone(),
        }))
    }
}
struct NoExternalAuthNetwork;
impl NetworkClient for NoExternalAuthNetwork {
    async fn send(
        &mut self,
        _: &ironrdp::connector::sspi::generator::NetworkRequest,
    ) -> ironrdp::connector::ConnectorResult<Vec<u8>> {
        Err(ironrdp::connector::general_err!(
            "external authentication transport is not enabled in this fixture"
        ))
    }
}
async fn client(
    address: std::net::SocketAddr,
    cert: rustls::pki_types::CertificateDer<'static>,
    events: Arc<Mutex<Vec<String>>>,
) -> anyhow::Result<()> {
    let tcp = tokio::net::TcpStream::connect(address).await?;
    let local = tcp.local_addr()?;
    let mut cfg = config::build_config(
        "fixture-user".into(),
        "fixture-password".into(),
        None,
        false,
        0,
    )?;
    cfg.enable_tls = true;
    cfg.enable_credssp = true;
    cfg.desktop_size.width = 640;
    cfg.desktop_size.height = 480;
    // Advertise only the lossless baseline under evaluation. Server-only optional
    // codec features otherwise unify PDU feature flags without client decoders.
    cfg.bitmap = Some(ironrdp::connector::BitmapConfig {
        lossy_compression: false,
        color_depth: 32,
        codecs: ironrdp::pdu::rdp::capability_sets::BitmapCodecs(vec![]),
    });
    let mut connector = ironrdp::connector::ClientConnector::new(cfg, local);
    let mut framed = TokioFramed::new(tcp);
    let upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert)?;
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.resumption = rustls::client::Resumption::disabled();
    let tls = tokio_rustls::TlsConnector::from(Arc::new(tls))
        .connect(
            "localhost".to_string().try_into()?,
            framed.into_inner_no_leftover(),
        )
        .await?;
    let peer = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|p| p.first())
        .ok_or_else(|| anyhow::anyhow!("no certificate"))?;
    use x509_cert::der::Decode;
    let cert = x509_cert::Certificate::from_der(peer)?;
    let public = cert
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .as_bytes()
        .unwrap()
        .to_vec();
    let upgraded = ironrdp_tokio::mark_as_upgraded(upgrade, &mut connector);
    let mut framed = TokioFramed::new(tls);
    let result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoExternalAuthNetwork,
        "localhost".into(),
        public,
        None,
    )
    .await?;
    let mut image = DecodedImage::new(
        ImageFormat::RgbA32,
        result.desktop_size.width,
        result.desktop_size.height,
    );
    let mut active = ActiveStageBuilder {
        static_channels: result.static_channels,
        user_channel_id: result.user_channel_id,
        io_channel_id: result.io_channel_id,
        message_channel_id: result.message_channel_id,
        share_id: result.share_id,
        compression_type: result.compression_type,
        enable_server_pointer: result.enable_server_pointer,
        pointer_software_rendering: result.pointer_software_rendering,
    }
    .build();
    let mut sent = false;
    loop {
        let (action, payload) = framed.read_pdu().await?;
        for output in active.process(&mut image, action, &payload)? {
            match output {
                ActiveStageOutput::ResponseFrame(frame) => framed.write_all(&frame).await?,
                ActiveStageOutput::GraphicsUpdate(_) => {
                    if &image.data()[..4] != &[255, 0, 0, 255] {
                        continue;
                    }
                    assert_eq!(&image.data()[..4], &[255, 0, 0, 255]);
                    if !sent {
                        for output in active.process_fastpath_input(
                            &mut image,
                            &[
                                FastPathInputEvent::MouseEvent(MousePdu {
                                    flags: PointerFlags::MOVE,
                                    number_of_wheel_rotation_units: 0,
                                    x_position: 45,
                                    y_position: 27,
                                }),
                                FastPathInputEvent::KeyboardEvent(KeyboardFlags::empty(), 0x1e),
                                FastPathInputEvent::KeyboardEvent(KeyboardFlags::RELEASE, 0x1e),
                            ],
                        )? {
                            if let ActiveStageOutput::ResponseFrame(frame) = output {
                                framed.write_all(&frame).await?;
                            }
                        }
                        sent = true;
                    }
                }
                _ => {}
            }
        }
        let seen = events.lock().unwrap();
        if seen.iter().any(|e| e == "mouse:Move { x: 45, y: 27 }")
            && seen
                .iter()
                .any(|e| e == "key:Pressed { code: 30, extended: false }")
        {
            println!(
                "actual_frame_rgba=[255,0,0,255]; actual_pointer=(45,27); actual_key=30; TLS_chain_and_name_checked=true"
            );
            return Ok(());
        }
    }
}
async fn run() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("provider already installed"))?;
    let root = tempfile::tempdir()?;
    let cert = root.path().join("cert.pem");
    let key = root.path().join("key.pem");
    let status = std::process::Command::new("openssl")
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;
    assert!(
        status.status.success(),
        "TLS fixture certificate generation failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let identity = TlsIdentityCtx::init_from_paths(&cert, &key)?;
    let root_cert = identity.certs[0].clone();
    let public = identity.pub_key;
    let tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(identity.certs, identity.priv_key)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let handler = Handler::default();
    let events = handler.0.clone();
    let mut server = tokio::task::spawn_local(async move {
        let (stream, _) = listener.accept().await?;
        let mut server = server::RdpServer::builder()
            .with_addr(address)
            .with_hybrid(tokio_rustls::TlsAcceptor::from(Arc::new(tls)), public)
            .with_input_handler(handler.clone())
            .with_display_handler(handler)
            .build();
        server.set_credentials(Some(server::Credentials {
            username: "fixture-user".into(),
            password: "fixture-password".into(),
            domain: None,
        }));
        server.run_connection(stream).await
    });
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        client(address, root_cert, events.clone()),
    )
    .await;
    if result.is_err() {
        eprintln!(
            "fixture_input={:?}; server_finished={}",
            events.lock().unwrap(),
            server.is_finished()
        );
    }
    let result = result?;
    if result.is_err() {
        server.abort();
        let _ = server.await;
        return result;
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(2), &mut server)
            .await
            .is_ok(),
        "Owned RDP server did not observe the client closing its transport"
    );
    println!("owned_server_connection_closed=true");
    result
}

fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--server-fixture") {
        return fixture();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tokio::task::LocalSet::new().run_until(run()))
}

fn fixture() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let root = std::path::PathBuf::from(
        args.get(2)
            .ok_or_else(|| anyhow::anyhow!("missing owned fixture root"))?,
    );
    let address: std::net::SocketAddr = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("127.0.0.1:0")
        .parse()?;
    assert!(address.ip().is_loopback());
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("provider already installed"))?;
    tls_fixture::prepare_localhost_identity(&root);
    let cert = root.join("server.pem");
    let key = root.join("server.key");
    let cursor = args.get(4).is_some_and(|s| s == "cursor");
    let resize = args.get(4).is_some_and(|s| s == "resize");
    let ignore_layout = args.get(4).is_some_and(|s| s == "ignore-layout");
    let shrink = args.get(4).is_some_and(|s| s == "shrink");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
        let identity = TlsIdentityCtx::init_from_paths(&cert, &key)?;
        let public = identity.pub_key;
        let tls = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(identity.certs, identity.priv_key)?,
        );
        let listener = tokio::net::TcpListener::bind(address).await?;
        use std::io::Write;
        println!("READY {}", listener.local_addr()?);
        std::io::stdout().flush()?;
        loop {
            let (stream, _) = listener.accept().await?;
            let root = root.clone();
            let public = public.clone();
            let tls = tls.clone();
            tokio::task::spawn_local(async move {
                let handler = Handler(
                    Default::default(),
                    Some(root.join("observations.txt")),
                    cursor,
                    Some(Arc::new(Mutex::new(server::DesktopSize {
                        width: if shrink { 800 } else { 640 },
                        height: if shrink { 600 } else { 480 },
                    }))),
                    if shrink {
                        Some(server::DesktopSize {
                            width: 640,
                            height: 480,
                        })
                    } else if resize {
                        Some(server::DesktopSize {
                            width: 800,
                            height: 600,
                        })
                    } else {
                        None
                    },
                    (!ignore_layout).then(Default::default),
                );
                let events = handler.0.clone();
                let mut server = server::RdpServer::builder()
                    .with_addr(address)
                    .with_hybrid(tokio_rustls::TlsAcceptor::from(tls), public)
                    .with_input_handler(handler.clone())
                    .with_display_handler(handler)
                    .with_cliprdr_factory(Some(Box::new(clipboard::Factory {
                        sender: None,
                        path: root.join("observations.txt"),
                    })))
                    .build();
                server.set_credentials(Some(server::Credentials {
                    username: "fixture-user".into(),
                    password: "fixture-password".into(),
                    domain: None,
                }));
                let result = server.run_connection(stream).await;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("observations.txt"))
                    .unwrap();
                for event in events.lock().unwrap().iter().take(256) {
                    writeln!(file, "{event}").unwrap();
                }
                writeln!(file, "CLOSED {}", result.is_ok()).unwrap();
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    }))
}
