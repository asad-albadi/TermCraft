use super::*;
#[test]
fn binary_websocket_bounds_and_masking() {
    let mask = [1, 2, 3, 4];
    let mut b = vec![0x82, 0x83];
    b.extend(mask);
    b.extend(b"abc".iter().enumerate().map(|(i, v)| v ^ mask[i % 4]));
    let (n, op, data) = frame(&b).unwrap().unwrap();
    assert_eq!((n, op, data), (9, 2, b"abc".to_vec()));
    assert!(frame(&[0x82, 0x7f, 0, 0, 0, 0, 0, 16, 0, 1]).is_err());
    assert!(frame(&[0x82, 0]).is_err());
    assert!(frame(&[0x02, 0x80]).is_err());
}
#[test]
fn browser_default_and_consent() {
    let mut c = VncServiceConfig::local(VncSource::WaylandPortal, Secret::new("fixture"));
    assert!(c.browser_bind.is_none());
    c.allow_unencrypted = true;
    c.browser_bind = Some("0.0.0.0:5800".parse().unwrap());
    assert!(c.validate().unwrap_err().contains("network access"));
}
#[test]
fn asset_paths_are_exact() {
    assert!(asset("/").is_some());
    assert!(asset("/novnc/core/rfb.js").is_some());
    for path in [
        "/?password=x",
        "/websockify?host=evil",
        "/novnc/../Cargo.toml",
        "/novnc/%2e%2e/",
        "/novnc/docs/API.md",
    ] {
        assert!(asset(path).is_none());
    }
}

pub(in crate::services::vnc) struct Synthetic {
    pub events: std::path::PathBuf,
}
impl Source for Synthetic {
    fn capture(&mut self) -> io::Result<Frame> {
        Ok(Frame {
            width: 64,
            height: 48,
            rgba: [19, 111, 173, 255].repeat(64 * 48),
        })
    }
    fn input(&mut self, event: Input) -> io::Result<()> {
        use std::io::Write;
        let text = match event {
            Input::Key { down, keysym } => format!("key {down} {keysym}\n"),
            Input::Pointer { buttons, x, y } => format!("pointer {buttons} {x} {y}\n"),
            Input::Release => "release\n".into(),
            Input::Clipboard(_) => "clipboard\n".into(),
        };
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.events)?
            .write_all(text.as_bytes())
    }
}
#[test]
#[ignore = "synthetic source only; driven by private-profile browser qualification"]
fn synthetic_browser_fixture() {
    use crate::services::{ServiceState, ServiceSupervisor};
    let path = std::path::PathBuf::from(
        std::env::var_os("TERMCRAFT_BROWSER_FIXTURE").expect("explicit fixture directory"),
    );
    assert!(path.is_absolute());
    let mut config = VncServiceConfig::local(
        VncSource::SyntheticFixture {
            events: path.join("events"),
        },
        Secret::new("fixture"),
    );
    config.allow_unencrypted = true;
    config.allow_control = std::env::var("TERMCRAFT_BROWSER_CONTROL").as_deref() == Ok("1");
    config.browser_bind = Some("127.0.0.1:0".parse().unwrap());
    config.lifetime = Some(Duration::from_secs(120));
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_vnc(config).unwrap();
    let status = supervisor.status(id).unwrap();
    std::fs::write(
        path.join("ready"),
        format!(
            "{}\n{}\n",
            status.address.unwrap(),
            status.browser_address.unwrap()
        ),
    )
    .unwrap();
    while !path.join("stop").exists()
        && !matches!(
            supervisor.status(id).unwrap().state,
            ServiceState::Stopped | ServiceState::Failed
        )
    {
        thread::sleep(Duration::from_millis(20));
    }
    let start = Instant::now();
    supervisor.stop(id).unwrap();
    while supervisor.status(id).unwrap().state != ServiceState::Stopped {
        assert!(
            start.elapsed() < Duration::from_secs(6),
            "{:?}",
            supervisor.status(id)
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(TcpStream::connect(status.address.unwrap()).is_err());
    assert!(TcpStream::connect(status.browser_address.unwrap()).is_err());
    std::fs::write(path.join("stopped"), "both listeners closed").unwrap();
}

#[test]
fn browser_bind_failure_rolls_back_owned_rfb_only() {
    use crate::services::ServiceSupervisor;
    let unrelated = TcpListener::bind("127.0.0.1:0").unwrap();
    let rfb = TcpListener::bind("127.0.0.1:0").unwrap();
    let rfb_address = rfb.local_addr().unwrap();
    drop(rfb);
    let dir = tempfile::tempdir().unwrap();
    let mut config = VncServiceConfig::local(
        VncSource::SyntheticFixture {
            events: dir.path().join("events"),
        },
        Secret::new("fixture"),
    );
    config.allow_unencrypted = true;
    config.bind = rfb_address;
    config.browser_bind = Some(unrelated.local_addr().unwrap());
    let supervisor = ServiceSupervisor::default();
    assert!(
        supervisor
            .start_vnc(config)
            .unwrap_err()
            .contains("browser viewer")
    );
    assert!(TcpStream::connect(rfb_address).is_err());
    assert!(TcpStream::connect(unrelated.local_addr().unwrap()).is_ok());
}
#[test]
fn disabled_browser_creates_no_endpoint() {
    use crate::services::{ServiceState, ServiceSupervisor};
    let dir = tempfile::tempdir().unwrap();
    let mut config = VncServiceConfig::local(
        VncSource::SyntheticFixture {
            events: dir.path().join("events"),
        },
        Secret::new("fixture"),
    );
    config.allow_unencrypted = true;
    let supervisor = ServiceSupervisor::default();
    let id = supervisor.start_vnc(config).unwrap();
    assert!(supervisor.status(id).unwrap().browser_address.is_none());
    supervisor.stop(id).unwrap();
    let deadline = Instant::now() + IO_LIMIT;
    while supervisor.status(id).unwrap().state != ServiceState::Stopped {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
}
