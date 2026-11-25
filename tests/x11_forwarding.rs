#![cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]

#[allow(dead_code)]
#[path = "support/xserver.rs"]
mod xserver;

#[allow(dead_code)]
#[path = "support/x11_ssh.rs"]
mod x11_ssh;

use harbor_workspace::ssh::{ConnectionCommand, ConnectionEvent};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use x11_ssh::Fixture;
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, CreateWindowAux, MapState, WindowClass},
};

const LIMIT: Duration = Duration::from_secs(5);

#[test]
#[ignore = "requires run_xserver_isolated.sh and the packaged Xwayland engine"]
fn forwarded_x_application_uses_owned_display() {
    let owned = xserver::start();
    let fixture = Fixture::start();
    let (_ssh, request) = fixture.connect(1, &owned);
    let client = fixture.x_client(&request, true).unwrap();
    let screen = &client.setup().roots[0];
    let window = client.generate_id().unwrap();
    client
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            screen.root,
            24,
            24,
            150,
            80,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.white_pixel),
        )
        .unwrap()
        .check()
        .unwrap();
    client.map_window(window).unwrap().check().unwrap();

    let observer = xserver::client(&owned, true).unwrap();
    let attributes = observer
        .get_window_attributes(window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(attributes.map_state, MapState::VIEWABLE);
    assert_eq!(fixture.logins.load(Ordering::Relaxed), 1);
}

#[test]
#[ignore = "requires run_xserver_isolated.sh and the packaged Xwayland engine"]
fn invalid_cookie_cannot_connect() {
    let owned = xserver::start();
    let fixture = Fixture::start();
    let (_ssh, request) = fixture.connect(1, &owned);
    assert!(fixture.x_client(&request, false).is_err());
    assert!(fixture.x_client(&request, true).is_ok());
}

#[test]
#[ignore = "requires run_xserver_isolated.sh and the packaged Xwayland engine"]
fn closing_session_removes_its_forwarding() {
    let owned = xserver::start();
    let fixture = Fixture::start();
    let (first, first_request) = fixture.connect(1, &owned);
    let (mut second, second_request) = fixture.connect(2, &owned);
    let first_client = fixture.x_client(&first_request, true).unwrap();
    let second_client = fixture.x_client(&second_request, true).unwrap();
    let local_client = xserver::client(&owned, true).unwrap();
    assert!(first_request.cookie.as_slice() != second_request.cookie.as_slice());

    first.cancel();
    let deadline = Instant::now() + LIMIT;
    while !first.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let root = first_client.setup().roots[0].root;
    assert!(
        first_client
            .get_geometry(root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_none()
    );
    assert!(xserver::window_is_visible_on_display(&second_client));
    assert!(xserver::window_is_visible_on_display(&local_client));
    assert!(fixture.x_client(&second_request, true).is_ok());

    let stopped_display = owned.client_display();
    owned.stop_owned().unwrap();
    assert!(stopped_display.environment().is_err());
    let root = second_client.setup().roots[0].root;
    assert!(
        second_client
            .get_geometry(root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_none()
    );
    second
        .try_send(ConnectionCommand::Input(b"SHELL_STILL_RUNNING".to_vec()))
        .unwrap();
    let deadline = Instant::now() + LIMIT;
    loop {
        for event in second.poll() {
            match event {
                ConnectionEvent::Output { bytes, .. } if bytes == b"SHELL_STILL_RUNNING" => return,
                ConnectionEvent::Exited { message, .. } => {
                    panic!("Stopping X11 ended SSH: {message}")
                }
                _ => {}
            }
        }
        assert!(
            Instant::now() < deadline,
            "SSH shell stopped responding after X11 stopped"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
