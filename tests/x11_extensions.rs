#![cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[allow(dead_code)]
#[path = "support/xserver.rs"]
mod support;
use harbor_workspace::x11::{XCapabilities, XServerLease};
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn probe(lease: &XServerLease, cancelled: &AtomicBool) -> Result<XCapabilities, String> {
    let executable = std::env::var_os("TERMCRAFT_TEST_X11_PROBE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_harbor-workspace")));
    XCapabilities::probe_with_executable(&lease.client_display(), &executable, cancelled)
}
fn report(lease: &XServerLease) -> XCapabilities {
    let report = probe(lease, &AtomicBool::new(false)).unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    report
}
#[test]
fn capability_report_does_not_turn_extension_presence_into_render_success() {
    let report: XCapabilities = serde_json::from_str(r#"{"glx_version":"1.4","opengl":null,"composite_version":"0.4","composite_pixels":false,"randr_version":"1.6","dimensions":[1280,720],"randr_change":null,"errors":["GLX context unavailable"]}"#).unwrap();
    assert!(!report.rendering_verified());
}
#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh"]
fn xcapabilities_reflect_real_server() {
    let lease = support::start();
    let report = report(&lease);
    assert_eq!(report.composite_version.as_deref(), Some("0.4"));
    assert!(report.composite_pixels, "{report:?}");
    assert!(report.randr_version.is_some());
    assert!(report.glx_version.is_some());
    assert!(report.dimensions[0] > 0);
    assert!(report.errors.is_empty(), "{report:?}");
}
#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh"]
fn known_gl_scene_renders_nonblank() {
    let lease = support::start();
    let report = report(&lease);
    assert!(report.rendering_verified(), "{report:?}");
    let gl = report.opengl.unwrap();
    assert_eq!(gl.matched_pixels, 4096);
    assert!(!gl.vendor.is_empty());
    assert!(!gl.renderer.is_empty());
    assert!(!gl.version.is_empty());
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(&gl.scene_png_base64)
        .unwrap();
    let image = image::load_from_memory(&png).unwrap();
    assert_eq!((image.width(), image.height()), (64, 64));
    let rgba = image.to_rgba8();
    for (x, y, expected) in [
        (16, 16, [255u8, 0, 0]),
        (48, 16, [0, 255, 0]),
        (16, 48, [0, 0, 255]),
        (48, 48, [255, 255, 0]),
    ] {
        assert!(
            rgba.get_pixel(x, y).0[..3]
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.abs_diff(expected) <= 3)
        );
    }
    if let Some(destination) = std::env::var_os("TERMCRAFT_TEST_X11_SCENE_PNG") {
        std::fs::write(destination, png).unwrap();
    }
    // A direct GLX context may still use llvmpipe; never infer GPU acceleration.
    if gl.renderer.to_ascii_lowercase().contains("llvmpipe") {
        assert!(gl.software);
    }
}
#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh"]
fn randr_resolution_change_is_observed() {
    use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};
    let lease = support::start();
    let report = report(&lease);
    assert!(report.initial_mode_restored);
    let changed = report
        .randr_change
        .expect("fixture must support an actual change and event");
    assert_ne!(changed, report.dimensions);
    let client = support::client(&lease, true).unwrap();
    let root = client.setup().roots[0].root;
    let geometry = client.get_geometry(root).unwrap().reply().unwrap();
    assert_eq!(
        [geometry.width as u32, geometry.height as u32],
        report.dimensions,
        "initial screen dimensions must be restored"
    );
}
#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh"]
fn cancelled_probe_keeps_owned_display_usable() {
    let mut lease = support::start();
    assert!(probe(&lease, &AtomicBool::new(true)).is_err());
    assert!(lease.is_running().unwrap());
    assert!(support::window_is_visible_on_display(
        &support::client(&lease, true).unwrap()
    ));
}

#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh"]
fn invalid_cookie_cannot_probe_owned_display() {
    use std::process::{Command, Stdio};
    let mut lease = support::start();
    let wrong = tempfile::NamedTempFile::new().unwrap();
    let mut record = std::fs::read(lease.authority_path()).unwrap();
    *record.last_mut().unwrap() ^= 0xff;
    std::fs::write(wrong.path(), record).unwrap();
    let executable = std::env::var_os("TERMCRAFT_TEST_X11_PROBE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_harbor-workspace")));
    let status = Command::new(executable)
        .arg("--termcraft-x11-capability-probe")
        .env("DISPLAY", lease.display())
        .env("XAUTHORITY", wrong.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(lease.is_running().unwrap());
    assert!(support::window_is_visible_on_display(
        &support::client(&lease, true).unwrap()
    ));
}

#[test]
#[ignore = "only through tests/support/run_xserver_isolated.sh; process cancellation, not graphics evidence"]
fn cancellation_reaps_a_stalled_probe_process() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, atomic::Ordering},
        time::{Duration, Instant},
    };
    let mut lease = support::start();
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("stalled-probe");
    let pid_file = directory.path().join("pid");
    std::fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec /bin/sleep 30\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let flag = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        worker_cancelled.store(true, Ordering::Relaxed);
    });
    let start = Instant::now();
    assert!(
        XCapabilities::probe_with_executable(&lease.client_display(), &executable, &cancelled)
            .is_err()
    );
    flag.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "stalled probe must be reaped"
    );
    assert!(lease.is_running().unwrap());
}
