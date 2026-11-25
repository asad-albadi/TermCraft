#[test]
fn packaging_manifest_and_release_contracts() {
    let python = std::env::var_os("TERMCRAFT_PYTHON")
        .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into());
    let status = std::process::Command::new(python)
        .args([
            "-m",
            "unittest",
            "discover",
            "-s",
            "tests",
            "-p",
            "packaging_*.py",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("Run the release packaging tests with Python 3.11 or newer");
    assert!(status.success(), "release packaging tests failed");
}
