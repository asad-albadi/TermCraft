#![cfg(unix)]
use harbor_workspace::{process::Process, terminal::Screen};
use std::time::{Duration, Instant};
fn pump(process: &mut Process, screen: &mut Screen) {
    for bytes in process.rx.try_iter() {
        screen.feed(&bytes);
    }
    let replies = std::mem::take(&mut screen.replies);
    if !replies.is_empty() {
        let _ = process.write(&replies);
    }
    process.poll();
}
fn wait(process: &mut Process, screen: &mut Screen, mut condition: impl FnMut(&Screen) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        pump(process, screen);
        if condition(screen) {
            return;
        }
        assert!(
            !process.exited,
            "Owned terminal fixture exited before expected state"
        );
        assert!(
            Instant::now() < deadline,
            "Owned terminal fixture never reached the expected state: rows={}, cols={}, cursor=({},{}), screen={:?}",
            screen.rows,
            screen.cols,
            screen.row,
            screen.col,
            screen.plain()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn vim_redraw_resize_and_exit_restore_the_primary_screen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owned-vim.txt");
    std::fs::write(&path, b"owned-vim-marker\nsecond line\n").unwrap();
    let args = [
        "-Nu",
        "NONE",
        "-i",
        "NONE",
        "-n",
        "--noplugin",
        path.to_str().unwrap(),
    ]
    .map(str::to_owned);
    let mut process = Process::spawn("/usr/bin/vim", &args, Some(root.path()), 80, 24).unwrap();
    let mut screen = Screen::new(80, 24, 1000);
    screen.feed(b"primary-guard\r\n");
    wait(&mut process, &mut screen, |s| {
        s.is_alternate() && s.plain().contains("owned-vim-marker")
    });
    assert_eq!(screen.cell(0, 0).unwrap().text, "o");
    assert!(screen.row < screen.rows && screen.col < screen.cols);
    screen.resize(100, 30);
    process.resize(100, 30);
    process.write(b"\x0c").unwrap();
    wait(&mut process, &mut screen, |s| {
        s.grid()[s.rows - 1]
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>()
            .contains("All")
    });
    process.write(b":q!\r").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process.exited {
        pump(&mut process, &mut screen);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    pump(&mut process, &mut screen);
    assert!(!screen.is_alternate());
    assert!(screen.plain().contains("primary-guard"));
}
#[test]
fn less_search_and_exit_restore_the_primary_screen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owned-less.txt");
    let mut text = String::new();
    for n in 0..300 {
        if n == 180 {
            text.push_str("owned-less-needle\n");
        } else {
            text.push_str(&format!("owned line {n}\n"));
        }
    }
    std::fs::write(&path, text).unwrap();
    let args = [
        "-u",
        "LESS",
        "-u",
        "LESSOPEN",
        "-u",
        "LESSCLOSE",
        "LESSHISTFILE=/dev/null",
        "LESSKEY=/dev/null",
        "LESSKEYIN=/dev/null",
        "LESSSECURE=1",
        "/usr/bin/less",
        "-R",
        "--",
        path.to_str().unwrap(),
    ]
    .map(str::to_owned);
    let mut process = Process::spawn("/usr/bin/env", &args, Some(root.path()), 80, 24).unwrap();
    let mut screen = Screen::new(80, 24, 1000);
    screen.feed(b"primary-guard\r\n");
    wait(&mut process, &mut screen, |s| {
        s.is_alternate() && s.plain().contains("owned line 0")
    });
    process.write(b"/owned-less-needle\r").unwrap();
    wait(&mut process, &mut screen, |s| {
        s.plain().contains("owned-less-needle")
    });
    process.write(b"q").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process.exited {
        pump(&mut process, &mut screen);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    pump(&mut process, &mut screen);
    assert!(!screen.is_alternate());
    assert!(screen.plain().contains("primary-guard"));
}
#[test]
#[ignore = "requires TERMCRAFT_TEST_HTOP pointing to the htop fixture executable"]
fn htop_owned_pid_redraw_resize_and_exit_restore_the_primary_screen() {
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let sleeper = OwnedChild(
        std::process::Command::new("/usr/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap(),
    );
    let root = tempfile::tempdir().unwrap();
    let executable =
        std::env::var("TERMCRAFT_TEST_HTOP").expect("Set the explicit htop fixture path");
    let config = format!("HTOPRC={}", root.path().join("htoprc").display());
    let pid = sleeper.0.id().to_string();
    let args = vec![
        config,
        executable,
        "--readonly".into(),
        "-C".into(),
        "-p".into(),
        pid,
    ];
    let mut process = Process::spawn("/usr/bin/env", &args, Some(root.path()), 110, 30).unwrap();
    let mut screen = Screen::new(110, 30, 1000);
    screen.feed(b"primary-guard\r\n");
    wait(&mut process, &mut screen, |s| {
        s.is_alternate() && s.plain().contains("sleep")
    });
    assert!(screen.row < screen.rows && screen.col < screen.cols);
    screen.resize(100, 26);
    process.resize(100, 26);
    process.write(b"\x0c").unwrap();
    wait(&mut process, &mut screen, |s| s.plain().contains("sleep"));
    process.write(b"q").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process.exited {
        pump(&mut process, &mut screen);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    pump(&mut process, &mut screen);
    assert!(!screen.is_alternate());
    assert!(screen.plain().contains("primary-guard"));
    drop(sleeper);
}
