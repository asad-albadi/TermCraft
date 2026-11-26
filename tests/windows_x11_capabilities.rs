#![cfg(all(windows, target_arch = "x86_64"))]

use harbor_workspace::x11::XCapabilities;
use std::process::Command;

#[test]
#[ignore = "requires an externally owned private Windows X server and Xauthority"]
fn authenticated_private_server_reports_live_extensions_and_composite_pixels() {
    assert!(std::env::var_os("DISPLAY").is_some());
    assert!(std::env::var_os("XAUTHORITY").is_some());
    let rejected_host = Command::new(env!("CARGO_BIN_EXE_harbor-workspace"))
        .arg("--termcraft-x11-capability-probe")
        .env(
            "DISPLAY",
            std::env::var("DISPLAY")
                .unwrap()
                .replacen("127.0.0.1:", "localhost:", 1),
        )
        .output()
        .unwrap();
    assert!(!rejected_host.status.success());
    assert!(
        String::from_utf8_lossy(&rejected_host.stderr).contains("127.0.0.1 loopback DISPLAY"),
        "{}",
        String::from_utf8_lossy(&rejected_host.stderr)
    );
    let rejected_authority = Command::new(env!("CARGO_BIN_EXE_harbor-workspace"))
        .arg("--termcraft-x11-capability-probe")
        .env_remove("XAUTHORITY")
        .output()
        .unwrap();
    assert!(!rejected_authority.status.success());
    assert!(
        String::from_utf8_lossy(&rejected_authority.stderr).contains("explicit XAUTHORITY"),
        "{}",
        String::from_utf8_lossy(&rejected_authority.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_harbor-workspace"))
        .arg("--termcraft-x11-capability-probe")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: XCapabilities = serde_json::from_slice(&output.stdout).unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    assert!(report.glx_version.is_some(), "{report:?}");
    assert!(report.opengl.is_none(), "indirect GLX must remain disabled");
    assert!(report.composite_version.is_some(), "{report:?}");
    assert!(report.composite_pixels, "{report:?}");
    assert!(report.randr_version.is_some(), "{report:?}");
    assert!(
        report.randr_change.is_none(),
        "no mode change was requested"
    );
    assert!(report.dimensions.iter().all(|dimension| *dimension > 0));
    assert!(report.initial_mode_restored);
}
