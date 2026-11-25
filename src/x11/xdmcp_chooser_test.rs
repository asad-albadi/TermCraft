use super::*;
use crate::x11::xdmcp::{ClipboardPolicy, QueryMode};
use std::{
    fs,
    process::{Child, Command, Stdio},
};

struct Managers(Child);

impl Drop for Managers {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let limit = Instant::now() + Duration::from_secs(6);
        while self.0.try_wait().ok().flatten().is_none() {
            if Instant::now() >= limit {
                let _ = self.0.kill();
                let _ = self.0.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn wait_for<T>(seconds: u64, expectation: &str, mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "{expectation}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn actual_chooser(clipboard: ClipboardPolicy) {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1")
    );
    let package = PathBuf::from(std::env::var("TERMCRAFT_TEST_XDMCP_PACKAGE").unwrap());
    let support = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support");
    let root = tempfile::tempdir().unwrap();
    let mut managers = Managers(
        Command::new("/usr/bin/python")
            .arg(support.join("xdmcp_chooser_managers.py"))
            .arg(root.path())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let initial = root.path().join("initial");
    let selected = root.path().join("selected");
    let manager = XServerManager::for_installation(package);
    let config = |display: u16,
                  chosen: std::net::SocketAddr,
                  mode: QueryMode,
                  clipboard: ClipboardPolicy| XdmcpConfig {
        selected_endpoint: Some(chosen),
        mode,
        display: Some(display),
        advertise_address: Some("192.0.2.1".parse().unwrap()),
        allow_network_listener: true,
        clipboard,
        clipboard_display: (clipboard == ClipboardPolicy::Bidirectional).then(|| ":0".into()),
        ..Default::default()
    };
    wait_for(4, "private xdm managers failed to listen", || {
        assert!(managers.0.try_wait().unwrap().is_none());
        let udp = fs::read_to_string("/proc/net/udp").unwrap();
        (udp.contains("0100007F:0499") && udp.contains("0200007F:00B1")).then_some(())
    });
    let cancel = AtomicBool::new(false);
    let mut companion = manager
        .start_xdmcp_cancellable(
            config(
                178,
                "127.0.0.2:177".parse().unwrap(),
                QueryMode::Direct,
                ClipboardPolicy::Disabled,
            ),
            640,
            480,
            &cancel,
        )
        .unwrap();
    wait_for(5, "independent direct session did not open", || {
        selected.join("managed-178").exists().then_some(())
    });
    let keyboard = if clipboard == ClipboardPolicy::Bidirectional {
        "de"
    } else {
        ""
    };
    let parent_map = super::tests::host_keyboard_map();
    let session_config = |n| {
        let mut options = config(
            n,
            "127.0.0.1:1177".parse().unwrap(),
            QueryMode::Indirect,
            clipboard,
        );
        options.keyboard = keyboard.into();
        options
    };
    let interact = |n: u16, action: &str| {
        let auth_dir = initial.join("auth/authdir/authfiles");
        let auth = wait_for(
            6,
            "initial xdm did not negotiate a chooser authority",
            || {
                fs::read_dir(&auth_dir)
                    .ok()?
                    .filter_map(Result::ok)
                    .find(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with(&format!("A192.0.2.1:{n}-"))
                    })
                    .map(|e| e.path())
            },
        );
        let result = Command::new("/usr/bin/python")
            .arg(support.join("xdmcp_chooser_select.py"))
            .arg(action)
            .env("DISPLAY", format!("192.0.2.1:{n}"))
            .env("XAUTHORITY", auth)
            .status()
            .unwrap();
        assert!(result.success(), "actual chooser interaction failed");
    };
    let cancel_session = manager
        .start_xdmcp_cancellable(session_config(176), 800, 600, &cancel)
        .unwrap();
    let pid = cancel_session.process_id().unwrap();
    let authority = cancel_session.authority_path().to_owned();
    interact(176, "observe");
    cancel_session.stop_owned().unwrap();
    assert!(!authority.exists() && !Path::new(&format!("/proc/{pid}")).exists());
    assert!(!Path::new("/tmp/.X11-unix/X176").exists());
    assert!(companion.is_running().unwrap());
    println!("Chooser cancellation removed only its own PID, authority and listener");

    let mut session = manager
        .start_xdmcp_cancellable(session_config(177), 800, 600, &cancel)
        .unwrap();
    let pid = session.process_id().unwrap();
    let authority = session.authority_path().to_owned();
    let marker = selected.join("managed-177");
    interact(177, "accept");
    wait_for(
        6,
        "selected xdm did not map its authenticated window",
        || {
            assert!(
                session.is_running().unwrap(),
                "engine exited after real chooser accepted a target"
            );
            marker.exists().then_some(())
        },
    );
    let receipt = fs::read_to_string(&marker).unwrap();
    assert!(
        receipt.contains("192.0.2.1:177")
            && receipt.contains("800x600")
            && receipt.contains("green pixel verified"),
        "{receipt}"
    );
    assert!(
        !initial.join("managed-177").exists(),
        "initial manager mapped a login, not the selected manager"
    );
    let observed = |dir: &Path, receipt: &[u8]| {
        fs::read(dir.join("log"))
            .unwrap()
            .windows(receipt.len())
            .any(|line| line == receipt)
    };
    // xdm's private debug stream can contain binary auth data. Match only the
    // protocol event labels and never print the stream.
    assert!(observed(&initial, b"Read from chooser successfully"));
    assert!(observed(&selected, b"Forward client address"));
    assert_eq!(session.process_id(), Some(pid));
    println!(
        "Real chooser on UDP 1177 accepted target, forwarded to UDP 177 and opened selected authenticated window: {receipt}"
    );
    if clipboard == ClipboardPolicy::Bidirectional {
        super::tests::verify_keyboard(&marker, keyboard, &parent_map);
        assert_eq!(
            session.ready_message,
            "XDMCP display session :177 established with bidirectional clipboard active"
        );
        let negotiated = fs::read_to_string(marker.with_extension("authority")).unwrap();
        super::super::xdmcp_clipboard::verify_managed(&session, Path::new(&negotiated));
        assert!(session.is_running().unwrap());
    }
    session.stop_owned().unwrap();
    assert!(!authority.exists() && !Path::new(&format!("/proc/{pid}")).exists());
    assert!(!Path::new("/tmp/.X11-unix/X177").exists());
    assert!(companion.is_running().unwrap());
    let host = std::env::var("TERMCRAFT_TEST_XSERVER_FIXTURE_PID").unwrap();
    assert!(Path::new(&format!("/proc/{host}")).exists());
    companion.stop_owned().unwrap();
}

#[test]
#[ignore = "real pinned xdm chooser, selected xdm and private X11/network namespaces"]
fn actual_indirect_chooser_handoff() {
    actual_chooser(ClipboardPolicy::Disabled);
}

#[test]
#[ignore = "real pinned xdm chooser, selected xdm and private X11/network namespaces"]
fn actual_indirect_chooser_handoff_with_clipboard() {
    actual_chooser(ClipboardPolicy::Bidirectional);
}
