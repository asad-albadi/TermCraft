//! Independent owner and requestor clients used only inside the disposable display namespace.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};
enum Command {
    Set(Vec<u8>, u8, bool, mpsc::Sender<()>),
    Clear(mpsc::Sender<()>),
}
struct Owner {
    window: u32,
    send: mpsc::Sender<Command>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<io::Result<()>>>,
}
impl Owner {
    fn open(display: &str, auth: Option<&Path>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (conn, _) = transport::parent(display, auth, stop.clone()).unwrap();
        let window = create_window(&conn).unwrap();
        let a = atoms(&conn);
        let (send, receive) = mpsc::channel();
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut stall = 0;
            let mut binary = false;
            let mut outgoing: Vec<(u32, u32, Vec<u8>, usize)> = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                transport::renew(&conn);
                for c in receive.try_iter() {
                    match c {
                        Command::Set(b, s, bin, ack) => {
                            bytes = b;
                            stall = s;
                            binary = bin;
                            conn.set_selection_owner(window, a.selection, 0u32)
                                .map_err(error)?
                                .check()
                                .map_err(error)?;
                            ack.send(()).unwrap();
                        }
                        Command::Clear(ack) => {
                            conn.set_selection_owner(0u32, a.selection, 0u32)
                                .map_err(error)?
                                .check()
                                .map_err(error)?;
                            ack.send(()).unwrap();
                        }
                    }
                }
                for _ in 0..64 {
                    match conn.poll_for_event().map_err(error)? {
                        Some(Event::SelectionRequest(e)) => {
                            if stall == 1 {
                                continue;
                            }
                            let outcome = (|| -> Result<(), x11rb::errors::ReplyError> {
                                let p = if e.property == 0 {
                                    e.target
                                } else {
                                    e.property
                                };
                                let mut accepted = 0;
                                if e.target == a.targets {
                                    conn.change_property32(
                                        PropMode::REPLACE,
                                        e.requestor,
                                        p,
                                        AtomEnum::ATOM,
                                        &[
                                            if binary {
                                                u32::from(AtomEnum::PIXMAP)
                                            } else {
                                                a.utf8
                                            },
                                            a.targets,
                                        ],
                                    )?
                                    .check()?;
                                    accepted = p;
                                } else if e.target == a.utf8 && !binary {
                                    if bytes.len() > CHUNK {
                                        conn.change_window_attributes(
                                            e.requestor,
                                            &ChangeWindowAttributesAux::new()
                                                .event_mask(EventMask::PROPERTY_CHANGE),
                                        )?
                                        .check()?;
                                        conn.change_property32(
                                            PropMode::REPLACE,
                                            e.requestor,
                                            p,
                                            a.incr,
                                            &[(bytes.len() + usize::from(stall == 3)) as u32],
                                        )?
                                        .check()?;
                                        outgoing.push((e.requestor, p, bytes.clone(), 0));
                                    } else {
                                        conn.change_property8(
                                            PropMode::REPLACE,
                                            e.requestor,
                                            p,
                                            a.utf8,
                                            &bytes,
                                        )?
                                        .check()?;
                                    }
                                    accepted = p;
                                }
                                conn.send_event(
                                    false,
                                    e.requestor,
                                    EventMask::NO_EVENT,
                                    SelectionNotifyEvent {
                                        response_type: SELECTION_NOTIFY_EVENT,
                                        sequence: 0,
                                        time: e.time,
                                        requestor: e.requestor,
                                        selection: e.selection,
                                        target: e.target,
                                        property: accepted,
                                    },
                                )?
                                .check()?;
                                Ok(())
                            })();
                            if let Err(e) = outcome {
                                if !matches!(&e,x11rb::errors::ReplyError::X11Error(e) if e.error_kind==x11rb::protocol::ErrorKind::Window)
                                {
                                    return Err(error(e));
                                }
                            }
                        }
                        Some(Event::PropertyNotify(e)) if e.state == Property::DELETE => {
                            if stall == 2 {
                                continue;
                            }
                            if let Some(index) = outgoing
                                .iter()
                                .position(|(w, p, _, _)| *w == e.window && *p == e.atom)
                            {
                                let (w, p, b, offset) = &mut outgoing[index];
                                let end = (*offset + 7001).min(b.len());
                                let done = *offset == b.len();
                                let okay = conn
                                    .change_property8(
                                        PropMode::REPLACE,
                                        *w,
                                        *p,
                                        a.utf8,
                                        &b[*offset..end],
                                    )
                                    .map_err(error)?
                                    .check()
                                    .is_ok();
                                *offset = end;
                                if done || !okay {
                                    outgoing.swap_remove(index);
                                }
                            }
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
                conn.flush().map_err(error)?;
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(())
        });
        Self {
            window,
            send,
            stop,
            thread: Some(thread),
        }
    }
    fn set(&self, text: &[u8]) {
        self.set_mode(text, false, false)
    }
    fn set_mode(&self, text: &[u8], stall: bool, binary: bool) {
        self.transfer_mode(text, u8::from(stall), binary);
    }
    fn transfer_mode(&self, text: &[u8], mode: u8, binary: bool) {
        let (s, r) = mpsc::channel();
        self.send
            .send(Command::Set(text.to_vec(), mode, binary, s))
            .unwrap();
        r.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    fn clear(&self) {
        let (s, r) = mpsc::channel();
        self.send.send(Command::Clear(s)).unwrap();
        r.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let result = h.join().unwrap();
            assert!(result.is_ok(), "fixture owner failed: {result:?}");
        }
    }
}
fn atoms(conn: &Conn) -> Atoms {
    let atom = |n: &[u8]| conn.intern_atom(false, n).unwrap().reply().unwrap().atom;
    Atoms {
        selection: atom(b"CLIPBOARD"),
        utf8: atom(b"UTF8_STRING"),
        targets: atom(b"TARGETS"),
        incr: atom(b"INCR"),
        property: atom(b"_X04_TEST_READ"),
        timestamp: atom(b"TIMESTAMP"),
    }
}
fn read(display: &str, auth: Option<&Path>) -> io::Result<Vec<u8>> {
    let (conn, _) = transport::parent(display, auth, Arc::new(AtomicBool::new(false)))?;
    let a = atoms(&conn);
    let window = create_window(&conn)?;
    conn.convert_selection(window, a.selection, a.utf8, a.property, 0u32)
        .map_err(error)?
        .check()
        .map_err(error)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut incr = false;
    let mut bytes = Vec::new();
    while Instant::now() < deadline {
        transport::renew(&conn);
        if let Some(e) = conn.poll_for_event().map_err(error)? {
            let retrieve = match e {
                Event::SelectionNotify(e) if e.requestor == window => {
                    if e.property == 0 {
                        return Err(error("No supported selection"));
                    }
                    true
                }
                Event::PropertyNotify(e)
                    if incr
                        && e.window == window
                        && e.atom == a.property
                        && e.state == Property::NEW_VALUE =>
                {
                    true
                }
                _ => false,
            };
            if retrieve {
                let p = conn
                    .get_property(
                        true,
                        window,
                        a.property,
                        AtomEnum::ANY,
                        0,
                        (MAX_TEXT / 4 + 1) as u32,
                    )
                    .map_err(error)?
                    .reply()
                    .map_err(error)?;
                if p.type_ == a.incr {
                    incr = true;
                    continue;
                }
                if p.type_ != a.utf8
                    || p.format != 8
                    || p.bytes_after != 0
                    || bytes.len() + p.value.len() > MAX_TEXT
                {
                    return Err(error("Invalid fixture read"));
                }
                let done = !incr || p.value.is_empty();
                bytes.extend_from_slice(&p.value);
                if done {
                    return Ok(bytes);
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    Err(error("Fixture read deadline"))
}
fn expect(display: &str, auth: Option<&Path>, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut observed = String::new();
    while Instant::now() < deadline {
        match read(display, auth) {
            Ok(b) if b == expected => return,
            Ok(b) => {
                observed = format!(
                    "{} bytes: {:?}",
                    b.len(),
                    String::from_utf8_lossy(&b[..b.len().min(60)])
                )
            }
            Err(e) => observed = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    panic!(
        "Expected {} bytes on {display}; observed {observed}",
        expected.len()
    )
}
fn owner(display: &str, auth: Option<&Path>) -> u32 {
    let (c, _) = transport::parent(display, auth, Arc::new(AtomicBool::new(false))).unwrap();
    c.get_selection_owner(atoms(&c).selection)
        .unwrap()
        .reply()
        .unwrap()
        .owner
}
fn expect_clear(display: &str, auth: Option<&Path>) {
    let start = Instant::now();
    while owner(display, auth) != 0 {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "clear not propagated to {display}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
#[ignore = "owned disposable display namespace only"]
fn actual_owned_bidirectional_clipboard_and_independent_leases() {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1")
    );
    let host_pid = std::env::var("TERMCRAFT_TEST_XSERVER_FIXTURE_PID").unwrap();
    let package = PathBuf::from(std::env::var("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap());
    let fixture = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_XDMCP_FIXTURE").unwrap());
    let dir = tempfile::tempdir().unwrap();
    let _dm = super::super::xdmcp_engine::tests::start_test_manager(dir.path(), &fixture);
    let host = Owner::open(":0", None);
    host.set(b"parent sentinel");
    let host_owner = owner(":0", None);
    let manager = crate::x11::XServerManager::for_installation(package);
    let config = |display, enabled| super::super::xdmcp::XdmcpConfig {
        selected_endpoint: Some(
            if enabled {
                "127.0.0.1:1177"
            } else {
                "127.0.0.1:1178"
            }
            .parse()
            .unwrap(),
        ),
        display: Some(display),
        advertise_address: Some("192.0.2.1".parse().unwrap()),
        allow_network_listener: true,
        clipboard: if enabled {
            super::super::xdmcp::ClipboardPolicy::Bidirectional
        } else {
            super::super::xdmcp::ClipboardPolicy::Disabled
        },
        clipboard_display: if enabled {
            Some(":0".into())
        } else {
            Some("invalid disabled display".into())
        },
        ..Default::default()
    };
    let mut disabled = manager
        .start_xdmcp_cancellable(config(177, false), 640, 480, &AtomicBool::new(false))
        .unwrap();
    let remote = Owner::open(disabled.display(), Some(disabled.authority_path()));
    remote.set(b"disabled remote");
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(owner(":0", None), host_owner);
    expect(":0", None, b"parent sentinel");
    let disabled_pid = disabled.process_id().unwrap();
    let disabled_auth = disabled.authority_path().to_owned();
    println!(
        "Disabled: parent selection owner and sentinel unchanged, malformed parent selection ignored; lease C remains active during enabled A/B sharing"
    );
    let mut first = manager
        .start_xdmcp_cancellable(config(178, true), 640, 480, &AtomicBool::new(false))
        .unwrap();
    let mut second = manager
        .start_xdmcp_cancellable(config(179, true), 640, 480, &AtomicBool::new(false))
        .unwrap();
    let a = (
        first.display().to_owned(),
        first.authority_path().to_owned(),
    );
    let b = (
        second.display().to_owned(),
        second.authority_path().to_owned(),
    );
    let remote_a = Owner::open(&a.0, Some(&a.1));
    let remote_b = Owner::open(&b.0, Some(&b.1));
    expect(&a.0, Some(&a.1), b"parent sentinel");
    expect(&b.0, Some(&b.1), b"parent sentinel");
    let text = "parent 🐧 café\nsecond line\n".as_bytes();
    host.set(text);
    expect(&a.0, Some(&a.1), text);
    expect(&b.0, Some(&b.1), text);
    host.set("same parent owner ✓\nupdated".as_bytes());
    expect(&a.0, Some(&a.1), "same parent owner ✓\nupdated".as_bytes());
    let back = "remote 日本語\nback to parent".as_bytes();
    remote_a.set(back);
    expect(":0", None, back);
    let parent_publication = owner(":0", None);
    let source_owner = remote_a.window;
    expect(&b.0, Some(&b.1), back);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        owner(":0", None),
        parent_publication,
        "lease B bounced the publication back to parent"
    );
    assert_eq!(
        owner(&a.0, Some(&a.1)),
        source_owner,
        "lease A received its own clipboard back"
    );
    println!(
        "Unicode/multiline both ways; same parent owner reacquisition updated; A -> parent -> B propagated once without ownership bounce"
    );
    remote_a.set("same remote owner 🦀".as_bytes());
    expect(":0", None, "same remote owner 🦀".as_bytes());
    expect(&b.0, Some(&b.1), "same remote owner 🦀".as_bytes());
    remote_a.clear();
    expect_clear(":0", None);
    expect_clear(&b.0, Some(&b.1));
    host.set(b"clear from parent");
    expect(&a.0, Some(&a.1), b"clear from parent");
    host.clear();
    expect_clear(&a.0, Some(&a.1));
    println!("Same remote owner update and conditional clear in both directions verified");
    let large = "α🐧\n".repeat(24000).into_bytes();
    host.set(&large);
    expect(&a.0, Some(&a.1), &large);
    expect(&b.0, Some(&b.1), &large);
    host.set(&large);
    host.set(b"latest epoch replaces pending large transfer");
    expect(
        &a.0,
        Some(&a.1),
        b"latest epoch replaces pending large transfer",
    );
    expect(
        &b.0,
        Some(&b.1),
        b"latest epoch replaces pending large transfer",
    );
    std::thread::sleep(Duration::from_millis(50));
    expect(
        &a.0,
        Some(&a.1),
        b"latest epoch replaces pending large transfer",
    );
    let large_back = "remote 🦀\n".repeat(18000).into_bytes();
    remote_a.set(&large_back);
    expect(":0", None, &large_back);
    println!(
        "Actual INCR: {} bytes parent -> bridge -> remote requestor; {} bytes remote -> bridge -> parent requestor",
        large.len(),
        large_back.len()
    );
    host.set(b"supported sentinel");
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.set_mode(b"binary", false, true);
    std::thread::sleep(Duration::from_millis(80));
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.set_mode(b"bad\0text", false, false);
    std::thread::sleep(Duration::from_millis(80));
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.set_mode(&vec![b'x'; MAX_TEXT + 1], false, false);
    std::thread::sleep(Duration::from_millis(80));
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.transfer_mode(&large, 3, false);
    std::thread::sleep(Duration::from_millis(120));
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.transfer_mode(&large, 2, false);
    std::thread::sleep(Duration::from_millis(2200));
    expect(&a.0, Some(&a.1), b"supported sentinel");
    host.set(b"recovered after INCR deadline");
    expect(&a.0, Some(&a.1), b"recovered after INCR deadline");
    host.set_mode(b"stall", true, false);
    std::thread::sleep(Duration::from_millis(80));
    let pid_a = first.process_id().unwrap();
    let pid_b = second.process_id().unwrap();
    assert!(first.is_running().unwrap());
    expect(
        disabled.display(),
        Some(disabled.authority_path()),
        b"disabled remote",
    );
    drop(remote_a);
    let start = Instant::now();
    first.stop_owned().unwrap();
    assert!(start.elapsed() < Duration::from_millis(800));
    assert!(second.is_running().unwrap());
    assert!(disabled.is_running().unwrap());
    assert!(!Path::new(&format!("/proc/{pid_a}")).exists());
    assert!(!a.1.exists());
    host.set(b"second survives stalled first close");
    expect(&b.0, Some(&b.1), b"second survives stalled first close");
    drop(remote_b);
    second.stop_owned().unwrap();
    assert!(!Path::new(&format!("/proc/{pid_b}")).exists());
    assert!(!b.1.exists());
    assert!(Path::new(&format!("/proc/{host_pid}")).exists());
    expect(
        disabled.display(),
        Some(disabled.authority_path()),
        b"disabled remote",
    );
    drop(remote);
    disabled.stop_owned().unwrap();
    assert!(!Path::new(&format!("/proc/{disabled_pid}")).exists());
    assert!(!disabled_auth.exists());
    println!(
        "Binary/NUL/oversize rejected without destination replacement; stalled owner cancellation bounded; A/B/C engine PIDs and private authority files cleaned; enabled B, Disabled C and parent survived A close; Disabled C text never changed"
    );
}

pub(super) fn verify_managed(session: &super::super::xdmcp_engine::XdmcpSession, authority: &Path) {
    let host = Owner::open(":0", None);
    host.set("actual XDMCP negotiated desktop 🐧\nparent clipboard".as_bytes());
    expect(
        session.display(),
        Some(authority),
        "actual XDMCP negotiated desktop 🐧\nparent clipboard".as_bytes(),
    );
    let remote = Owner::open(session.display(), Some(authority));
    remote.set("actual xdm client 🦀\nremote clipboard".as_bytes());
    expect(
        ":0",
        None,
        "actual xdm client 🦀\nremote clipboard".as_bytes(),
    );
    println!(
        "Actual xdm-negotiated authenticated client exchanged Unicode text in both directions with the retained bridge"
    );
}
