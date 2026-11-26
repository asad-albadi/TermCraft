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

#[test]
fn cmd_runs_a_quoted_output_path_through_conpty() {
    let temporary = tempfile::tempdir().unwrap();
    let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    let output_file = temporary.path().join("legacy command output.txt");
    let command = "echo TERMCRAFT_COMMAND>\"legacy command output.txt\"";
    let mut process = Process::spawn(
        cmd.to_str().unwrap(),
        &["/D".into(), "/C".into(), command.into()],
        Some(temporary.path()),
        80,
        25,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process.exited && Instant::now() < deadline {
        process.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    let output: Vec<_> = process.rx.try_iter().flatten().collect();
    assert_eq!(
        process.exit_message,
        "Process ended with code 0",
        "cmd output: {}",
        String::from_utf8_lossy(&output)
    );
    assert_eq!(
        std::fs::read_to_string(output_file).unwrap().trim(),
        "TERMCRAFT_COMMAND"
    );
}

#[test]
fn native_console_program_restores_primary_screen_after_fullscreen_resize() {
    use harbor_workspace::terminal::Screen;
    use std::io::{self, Write};
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };

    const CHILD: &str = "TERMCRAFT_TEST_CONPTY_FULLSCREEN_CHILD";
    const TEST: &str = "native_console_program_restores_primary_screen_after_fullscreen_resize";
    if std::env::var_os(CHILD).is_some() {
        let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        let mut mode = 0;
        assert_ne!(unsafe { GetConsoleMode(output, &mut mode) }, 0);
        assert_ne!(
            unsafe { SetConsoleMode(output, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) },
            0
        );
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"TC_PRIMARY\r\n").unwrap();
        stdout.flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        assert_eq!(input.trim(), "enter");
        stdout.write_all(b"\x1b[?1049hTC_FULLSCREEN\r\n").unwrap();
        stdout.flush().unwrap();
        input.clear();
        io::stdin().read_line(&mut input).unwrap();
        assert_eq!(input.trim(), "leave");
        stdout.write_all(b"\x1b[?1049lTC_RESTORED\r\n").unwrap();
        stdout.flush().unwrap();
        return;
    }

    fn wait_for_screen(process: &Process, screen: &mut Screen, marker: &str) {
        let deadline = Instant::now() + Duration::from_secs(12);
        while !screen.plain().contains(marker) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let chunk = process.rx.recv_timeout(remaining).unwrap_or_else(|error| {
                panic!(
                    "ConPTY did not show {marker:?}: {error}; screen={}",
                    screen.plain()
                )
            });
            screen.feed(&chunk);
        }
    }

    let temporary = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut process = Process::spawn_with_environment(
        executable.to_str().unwrap(),
        &["--exact".into(), TEST.into(), "--nocapture".into()],
        Some(temporary.path()),
        80,
        25,
        &[(CHILD.into(), "1".into())],
    )
    .unwrap();
    let mut screen = Screen::new(80, 25, 100);
    wait_for_screen(&process, &mut screen, "TC_PRIMARY");
    assert!(!screen.is_alternate());
    process.write(b"enter\r").unwrap();
    wait_for_screen(&process, &mut screen, "TC_FULLSCREEN");
    assert!(screen.is_alternate());
    assert!(!screen.plain().contains("TC_PRIMARY"));
    process.resize(100, 32);
    screen.resize(100, 32);
    process.write(b"leave\r").unwrap();
    wait_for_screen(&process, &mut screen, "TC_RESTORED");
    assert!(!screen.is_alternate());
    assert!(screen.plain().contains("TC_PRIMARY"));
    assert!(!screen.plain().contains("TC_FULLSCREEN"));
}
