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
            if kind == 2 {
                if u16::from_be_bytes(bytes[..2].try_into().unwrap()) == width
                    && u16::from_be_bytes(bytes[2..4].try_into().unwrap()) == height
                {
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
