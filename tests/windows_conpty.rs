#![cfg(windows)]

use harbor_workspace::process::Process;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn wait_for(process: &Process, output: &mut Vec<u8>, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !String::from_utf8_lossy(output).contains(marker) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let chunk = process.rx.recv_timeout(remaining).unwrap_or_else(|error| {
            panic!(
                "ConPTY did not emit {marker:?}: {error}; output={}",
                String::from_utf8_lossy(output)
            )
        });
        output.extend(chunk);
    }
}

#[test]
fn powershell_round_trips_unicode_after_conpty_resize() {
    let temporary = tempfile::tempdir().unwrap();
    let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let script = "[Console]::InputEncoding=[Text.Encoding]::UTF8; \
                  [Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                  [Console]::WriteLine('TC_READY'); \
                  $line=[Console]::ReadLine(); \
                  [Console]::WriteLine('TC_SIZE:'+([Console]::WindowWidth)+'x'+([Console]::WindowHeight)); \
                  [Console]::WriteLine('TC_REPLY:'+$line)";
    let args = [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]
    .map(str::to_owned);
    let mut process = Process::spawn(
        powershell.to_str().unwrap(),
        &args,
        Some(temporary.path()),
        80,
        25,
    )
    .unwrap();
    let mut output = Vec::new();
    wait_for(&process, &mut output, "TC_READY");
    process.resize(100, 32);
    process.write("café 日本\r".as_bytes()).unwrap();
    wait_for(&process, &mut output, "TC_SIZE:100x32");
    wait_for(&process, &mut output, "TC_REPLY:café 日本");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process.exited && Instant::now() < deadline {
        process.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(process.exited, "owned PowerShell did not exit");
}
