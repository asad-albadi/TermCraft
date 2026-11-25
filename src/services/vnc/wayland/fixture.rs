//! Private-bus portal mock plus real native PipeWire transport. Never host consent.
use super::*;
use std::{
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};
use zbus::{
    Connection,
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};
#[derive(Default)]
struct State {
    mode: AtomicU8,
    log: Mutex<Vec<String>>,
    notify: Mutex<Vec<(String, Vec<f64>)>>,
}
#[derive(Clone)]
struct Mock {
    state: Arc<State>,
    node: u32,
    serial: u64,
}
impl Mock {
    async fn response(
        &self,
        conn: &Connection,
        hdr: &Header<'_>,
        opts: portal::Dict,
        method: &str,
        results: portal::Dict,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = hdr.sender().unwrap().as_str();
        let token = <&str>::try_from(opts.get("handle_token").unwrap()).unwrap();
        let path = OwnedObjectPath::try_from(format!(
            "/org/freedesktop/portal/desktop/request/{}/{}",
            sender.trim_start_matches(':').replace('.', "_"),
            token
        ))
        .unwrap();
        conn.object_server()
            .at(
                path.clone(),
                Request {
                    state: self.state.clone(),
                },
            )
            .await?;
        self.state.log.lock().unwrap().push(method.into());
        if !(method == "Start" && self.state.mode.load(Ordering::Acquire) == 2) {
            let code = if method == "Start" && self.state.mode.load(Ordering::Acquire) == 1 {
                1u32
            } else {
                0
            };
            conn.emit_signal(
                Some(sender),
                path.clone(),
                "org.freedesktop.portal.Request",
                "Response",
                &(code, results),
            )
            .await?;
        }
        Ok(path)
    }
    async fn create(
        &self,
        conn: &Connection,
        hdr: Header<'_>,
        opts: portal::Dict,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = <&str>::try_from(opts.get("session_handle_token").unwrap()).unwrap();
        let sender = hdr.sender().unwrap().as_str();
        let path = format!(
            "/org/freedesktop/portal/desktop/session/{}/{}",
            sender.trim_start_matches(':').replace('.', "_"),
            token
        );
        conn.object_server()
            .at(
                path.clone(),
                Session {
                    state: self.state.clone(),
                },
            )
            .await?;
        self.response(
            conn,
            &hdr,
            opts,
            "CreateSession",
            portal::Dict::from([(
                "session_handle".into(),
                Value::from(path).try_into().unwrap(),
            )]),
        )
        .await
    }
    async fn start(
        &self,
        conn: &Connection,
        hdr: Header<'_>,
        opts: portal::Dict,
        control: bool,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let metadata = portal::Dict::from([
            ("pipewire-serial".into(), OwnedValue::from(self.serial)),
            (
                "logical_size".into(),
                Value::from((640i32, 360i32)).try_into().unwrap(),
            ),
        ]);
        let mut values = portal::Dict::from([(
            "streams".into(),
            Value::from(vec![(self.node, metadata)]).try_into().unwrap(),
        )]);
        if control {
            values.insert(
                "devices".into(),
                OwnedValue::from(if self.state.mode.load(Ordering::Acquire) == 3 {
                    1u32
                } else {
                    3
                }),
            );
        }
        self.response(conn, &hdr, opts, "Start", values).await
    }
}
struct ScreenCast(Mock);
#[zbus::interface(name = "org.freedesktop.portal.ScreenCast")]
impl ScreenCast {
    #[zbus(property)]
    fn available_source_types(&self) -> u32 {
        if self.0.state.mode.load(Ordering::Acquire) == 4 {
            0
        } else {
            3
        }
    }

    async fn create_session(
        &self,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.create(conn, hdr, opts).await
    }
    async fn select_sources(
        &self,
        _session: OwnedObjectPath,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(
            bool::try_from(opts.get("multiple").unwrap()).unwrap(),
            false
        );
        assert!(!opts.contains_key("restore_token"));
        assert!(!opts.contains_key("persist_mode"));
        self.0
            .response(conn, &hdr, opts, "SelectSources", portal::Dict::new())
            .await
    }
    async fn start(
        &self,
        _session: OwnedObjectPath,
        parent: &str,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert!(parent.is_empty());
        self.0.start(conn, hdr, opts, false).await
    }
    fn open_pipe_wire_remote(
        &self,
        _session: OwnedObjectPath,
        _opts: portal::Dict,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
        self.0
            .state
            .log
            .lock()
            .unwrap()
            .push("OpenPipeWireRemote".into());
        let path = std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap())
            .join("e03-private-pipewire");
        Ok(std::os::fd::OwnedFd::from(UnixStream::connect(path).unwrap()).into())
    }
}
struct Remote(Mock);
#[zbus::interface(name = "org.freedesktop.portal.RemoteDesktop")]
impl Remote {
    #[zbus(property)]
    fn available_device_types(&self) -> u32 {
        3
    }

    async fn create_session(
        &self,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.create(conn, hdr, opts).await
    }
    async fn select_devices(
        &self,
        _session: OwnedObjectPath,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(u32::try_from(opts.get("types").unwrap()).unwrap(), 3);
        self.0
            .response(conn, &hdr, opts, "SelectDevices", portal::Dict::new())
            .await
    }
    async fn start(
        &self,
        _session: OwnedObjectPath,
        _parent: &str,
        opts: portal::Dict,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.start(conn, hdr, opts, true).await
    }
    fn notify_keyboard_keysym(
        &self,
        _session: OwnedObjectPath,
        _opts: portal::Dict,
        key: i32,
        state: u32,
    ) {
        self.0
            .state
            .notify
            .lock()
            .unwrap()
            .push(("key".into(), vec![f64::from(key), f64::from(state)]));
    }
    fn notify_pointer_motion_absolute(
        &self,
        _session: OwnedObjectPath,
        _opts: portal::Dict,
        node: u32,
        x: f64,
        y: f64,
    ) {
        assert_eq!(node, self.0.node);
        self.0
            .state
            .notify
            .lock()
            .unwrap()
            .push(("motion".into(), vec![x, y]));
    }
    fn notify_pointer_button(
        &self,
        _session: OwnedObjectPath,
        _opts: portal::Dict,
        button: i32,
        state: u32,
    ) {
        self.0
            .state
            .notify
            .lock()
            .unwrap()
            .push(("button".into(), vec![f64::from(button), f64::from(state)]));
    }
    fn notify_pointer_axis_discrete(
        &self,
        _session: OwnedObjectPath,
        _opts: portal::Dict,
        axis: u32,
        steps: i32,
    ) {
        self.0
            .state
            .notify
            .lock()
            .unwrap()
            .push(("wheel".into(), vec![f64::from(axis), f64::from(steps)]));
    }
}
struct Request {
    state: Arc<State>,
}
#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl Request {
    fn close(&self) {
        self.state.log.lock().unwrap().push("Request.Close".into());
    }
}
struct Session {
    state: Arc<State>,
}
#[zbus::interface(name = "org.freedesktop.portal.Session")]
impl Session {
    fn close(&self) {
        self.state.log.lock().unwrap().push("Session.Close".into());
    }
}
#[test]
#[ignore = "requires explicit private D-Bus and PipeWire synthetic-source wrapper; never host screen or input"]
fn private_portal_real_pipewire_frames_control_denial_and_cancel() {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_WAYLAND_ISOLATED").as_deref(),
        Ok("1")
    );
    assert!(std::env::var_os("DISPLAY").is_none());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    let state = Arc::new(State::default());
    let mock = Mock {
        state: state.clone(),
        node: std::env::var("E03_NODE").unwrap().parse().unwrap(),
        serial: std::env::var("E03_SERIAL").unwrap().parse().unwrap(),
    };
    let running = Arc::new(AtomicBool::new(true));
    let run = running.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let conn = zbus::connection::Builder::session()
                .unwrap()
                .name("org.freedesktop.portal.Desktop")
                .unwrap()
                .serve_at("/org/freedesktop/portal/desktop", ScreenCast(mock.clone()))
                .unwrap()
                .serve_at("/org/freedesktop/portal/desktop", Remote(mock))
                .unwrap()
                .build()
                .await
                .unwrap();
            ready_tx.send(conn.clone()).unwrap();
            while run.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            drop(conn);
        });
    });
    let portal_server = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let install = tempfile::tempdir().unwrap();
    let helper = std::path::PathBuf::from(std::env::var_os("E03_HELPER").unwrap());
    let bytes = std::fs::read(&helper).unwrap();
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    std::fs::copy(helper, install.path().join("termcraft-wayland-engine")).unwrap();
    let manifest = serde_json::json!({"schema_version":1,"platform":"linux","architecture":std::env::consts::ARCH,"rdp_helper_version":"unused","wayland_helper_version":"0.1.0","files":{"termcraft-wayland-engine":digest}});
    std::fs::write(
        install.path().join("PACKAGE-MANIFEST.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let open = |control, cancel| Wayland::start(control, cancel, Some(install.path()));
    // Real fd remote and native mapped frames from daemon-owned video test source.
    let mut source = open(false, Arc::new(|| false)).unwrap();
    let a = source.capture().unwrap();
    assert!(!a.rgba.is_empty());
    assert!(a.rgba.chunks_exact(4).all(|p| p[3] == 255));
    assert!(
        a.rgba
            .chunks_exact(4)
            .any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
    );
    std::thread::sleep(Duration::from_millis(200));
    let b = source.capture().unwrap();
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert!(
        a.rgba != b.rgba,
        "synthetic source must deliver changing actual native frames"
    );
    assert!(
        source
            .input(Input::Key {
                down: true,
                keysym: 65
            })
            .is_err()
    );
    assert!(state.notify.lock().unwrap().is_empty());
    drop(source);
    assert_eq!(
        state.log.lock().unwrap().as_slice(),
        [
            "CreateSession",
            "SelectSources",
            "Start",
            "OpenPipeWireRemote",
            "Session.Close"
        ]
    );
    state.log.lock().unwrap().clear();
    let mut source = open(true, Arc::new(|| false)).unwrap();
    let f = source.capture().unwrap();
    source
        .input(Input::Key {
            down: true,
            keysym: 0x20ac,
        })
        .unwrap();
    source
        .input(Input::Key {
            down: false,
            keysym: 0x010020ac,
        })
        .unwrap();
    source
        .input(Input::Key {
            down: true,
            keysym: 0xffe1,
        })
        .unwrap();
    source
        .input(Input::Pointer {
            buttons: 9,
            x: f.width / 2,
            y: f.height / 2,
        })
        .unwrap();
    source.input(Input::Release).unwrap();
    let messages = state.notify.lock().unwrap().clone();
    assert_eq!(&messages[0], &("key".into(), vec![0x20ac as f64, 1.]));
    assert_eq!(&messages[1], &("key".into(), vec![0x20ac as f64, 0.]));
    assert!(messages.contains(&("motion".into(), vec![320., 180.])));
    assert!(messages.contains(&("button".into(), vec![272., 0.])));
    assert!(messages.contains(&("wheel".into(), vec![0., -1.])));
    // Portal revocation retires the generation and terminates its owned native stream.
    let session = source.portal.session();
    source
        .portal
        .runtime
        .block_on(portal_server.emit_signal(
            None::<&str>,
            session,
            "org.freedesktop.portal.Session",
            "Closed",
            &(portal::Dict::new(),),
        ))
        .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert!(source.capture().is_err());
    assert!(
        source
            .input(Input::Key {
                down: true,
                keysym: 66
            })
            .is_err()
    );
    drop(source);
    assert!(state.log.lock().unwrap().contains(&"SelectDevices".into()));
    for mode in [1, 3, 4] {
        state.mode.store(mode, Ordering::Release);
        state.log.lock().unwrap().clear();
        assert!(open(mode == 3, Arc::new(|| false)).is_err());
        let log = state.log.lock().unwrap();
        assert!(log.contains(&"Session.Close".into()));
        assert!(!log.contains(&"OpenPipeWireRemote".into()));
    }
    // Stop during a pending consent request closes only its request and session.
    state.mode.store(2, Ordering::Release);
    state.log.lock().unwrap().clear();
    let cancelled = Arc::new(AtomicBool::new(false));
    let c = cancelled.clone();
    let install_path = install.path().to_owned();
    let worker = std::thread::spawn(move || {
        Wayland::start(
            false,
            Arc::new(move || c.load(Ordering::Acquire)),
            Some(&install_path),
        )
        .is_err()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state.log.lock().unwrap().contains(&"Start".into()) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = Instant::now();
    cancelled.store(true, Ordering::Release);
    assert!(worker.join().unwrap());
    assert!(start.elapsed() < Duration::from_secs(2));
    let log = state.log.lock().unwrap().clone();
    assert!(log.contains(&"Request.Close".into()));
    assert!(log.contains(&"Session.Close".into()));
    assert!(!log.contains(&"OpenPipeWireRemote".into()));
    running.store(false, Ordering::Release);
    server.join().unwrap();
}
