//! Drives the real helper's private pipe to reproduce input queued before a resize.
#[path = "../../src/desktop/options.rs"]
mod rdp_options;
#[allow(dead_code)]
#[path = "../../src/desktop/wire.rs"]
pub mod wire;
use std::{
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub struct Client {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<(u8, zeroize::Zeroizing<Vec<u8>>)>,
    canvas: Option<(u16, u16, u64)>,
}
impl Client {
    pub fn connect(server: &super::rdp_server::Server) -> Self {
        let mut child = Command::new(super::rdp_server::engine_binary())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::sync_channel(4);
        std::thread::spawn(move || {
            while let Ok(packet) = wire::read_packet(&mut output) {
                if tx.send(packet).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            events,
            canvas: None,
        };
        client.send(wire::Input::Configure(wire::Config {
            version: wire::VERSION,
            options: Default::default(),
            connect_to: None,
            carrier_token: None,
            host: "localhost".into(),
            port: server.address.port(),
            user: "fixture-user".into(),
            width: 800,
            height: 600,
            timeout_secs: 10,
            certificate_authority: Some(server.root.path().join("ca.pem")),
            remembered_certificate: None,
        }));
        client
    }
    pub fn send(&mut self, input: wire::Input) {
        wire::write_json(&mut self.input, &input).unwrap();
    }
    pub fn frame(&mut self, width: u16, height: u16) {
        let end = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            assert!(
                std::time::Instant::now() < end,
                "Timed out waiting for helper frame"
            );
            let (kind, bytes) = self
                .events
                .recv_timeout(Duration::from_secs(3))
                .expect("RDP helper ended");
            if kind == 2 || kind == 3 {
                let header = if kind == 2 { 12 } else { 20 };
                assert!(bytes.len() >= header, "Truncated desktop frame header");
                let word = |at| u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
                let extent = (word(0), word(2));
                let revision = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
                let payload = if kind == 2 {
                    assert_ne!(revision, 0, "Missing full-frame revision");
                    wire::dimensions(extent.0, extent.1).unwrap()
                } else {
                    let (cw, ch, previous) = self.canvas.expect("Patch has no baseline");
                    assert_eq!(extent, (cw, ch), "Patch changed the canvas");
                    assert_eq!(
                        Some(revision),
                        previous.checked_add(1),
                        "Patch revision gap"
                    );
                    wire::patch_bounds(cw, ch, word(12), word(14), word(16), word(18)).unwrap()
                };
                assert_eq!(
                    bytes.len(),
                    header + payload,
                    "Invalid frame payload length"
                );
                self.canvas = Some((extent.0, extent.1, revision));
                if extent == (width, height) {
                    return;
                }
            } else {
                match serde_json::from_slice::<wire::Event>(&bytes).unwrap() {
                    wire::Event::Authentication { id, .. } => {
                        self.send(wire::Input::Authenticate {
                            id,
                            user: "fixture-user".into(),
                            password: "fixture-password".into(),
                            domain: String::new(),
                        })
                    }
                    wire::Event::Disconnected { message } => panic!("{message}"),
                    _ => {}
                }
            }
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
