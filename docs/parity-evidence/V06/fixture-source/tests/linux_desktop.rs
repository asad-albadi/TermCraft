//! Evidence validators for the separate native-session runner, never native substitutes.
//! Set V06_FIXTURE_ROOT to the pinned private fixture and first run check-linux-desktop.sh --run.
#![cfg(target_os = "linux")]
use std::{path::PathBuf, process::Command};
fn fixture() -> PathBuf {
    std::env::var_os("V06_FIXTURE_ROOT").expect("native V06 fixture must be explicit; do not run against a host desktop").into()
}
fn evidence(case: &str, expected: usize) {
    let root = fixture();
    let result = Command::new("python").arg("-I").arg(root.join("verify.py")).arg(root.join("package-reports")).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let mut count = 0;
    for backend in ["x11", "wayland"] {
        for scale in ["1", "1.5", "2"] {
            let data = std::fs::read(root.join("package-reports").join(format!("{backend}-{scale}/observations.json"))).unwrap();
            let report: serde_json::Value = serde_json::from_slice(&data).unwrap();
            count += report["observations"].as_array().unwrap().iter().filter(|v| v["case"] == case).count();
        }
    }
    assert_eq!(count, expected);
}
#[test]
#[ignore = "requires separately executed pinned native Linux matrix; validator only"]
fn system_theme_changes_while_idle() { evidence("system_theme_changes_while_idle", 36); }
#[test]
#[ignore = "requires separately executed pinned native Linux matrix; validator only"]
fn clipboard_roundtrip_between_real_windows() { evidence("clipboard_roundtrip_between_real_windows", 12); }
#[test]
#[ignore = "requires separately executed pinned native Linux matrix; validator only"]
fn scaled_detached_views_keep_correct_input_coordinates() { evidence("scaled_detached_views_keep_correct_input_coordinates", 12); }
