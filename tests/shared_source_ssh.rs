#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    remote_files::RemotePath,
    ssh::{ConnectionEvent, ConnectionHandle},
};
use std::time::{Duration, Instant};
#[test]
fn bounded_source_sftp_read_does_not_start_a_terminal_or_execute_login_command() {
    let server = ssh_server::SshServer::start(true);
    let file = server.root.join("source.json");
    std::fs::write(&file, b"source bytes").unwrap();
    let marker = server.root.join("must-not-start-shell");
    let mut config = server.config(9000, true);
    config.session.command = format!("touch '{}'; exec /bin/sh", marker.display());
    let mut handle = ConnectionHandle::read_file(
        config,
        1,
        RemotePath::from_bytes(file.to_str().unwrap().as_bytes().to_vec()).unwrap(),
        1024,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut result = None;
    while result.is_none() && Instant::now() < deadline {
        for event in handle.poll() {
            match event {
                ConnectionEvent::FileContents {
                    connection: 9000,
                    request: 1,
                    bytes,
                } => result = Some(bytes),
                ConnectionEvent::Exited { message, .. } if result.is_none() => panic!("{message}"),
                ConnectionEvent::Challenge { .. } => {
                    panic!("trusted fixture unexpectedly prompted")
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(result.unwrap(), b"source bytes");
    assert!(!marker.exists());
    assert_eq!(server.accepted_authentications(), 1);
}
#[test]
fn source_sftp_size_limit_never_returns_partial_definitions() {
    let server = ssh_server::SshServer::start(true);
    let file = server.root.join("too-large.json");
    std::fs::write(&file, vec![b'x'; 256]).unwrap();
    let mut handle = ConnectionHandle::read_file(
        server.config(9001, true),
        2,
        RemotePath::from_bytes(file.to_str().unwrap().as_bytes().to_vec()).unwrap(),
        32,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut failed = false;
    while !failed && Instant::now() < deadline {
        for event in handle.poll() {
            match event {
                ConnectionEvent::FileContents { .. } => {
                    panic!("oversized file returned as successful")
                }
                ConnectionEvent::Exited { message, .. } => {
                    assert!(message.contains("limit"), "{message}");
                    failed = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(failed);
}
