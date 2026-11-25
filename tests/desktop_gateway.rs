#[allow(dead_code)]
#[path = "support/vnc_server.rs"]
mod vnc_fixture;

#[allow(dead_code)]
#[path = "support/desktop_gateway_server.rs"]
mod gateway_fixture;

use gateway_fixture::{Fixture, TARGET, USER, WAIT, authenticate, fixture_key, pause};
use harbor_workspace::{
    model::{AgentMode, Session},
    ssh::{
        ConnectionEvent, TrustDecision,
        desktop_gateway::{Endpoint, GatewayLease},
    },
};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    sync::atomic::Ordering::SeqCst,
    time::Instant,
};

#[test]
fn gateway_reaches_target_without_public_listener() {
    let fixture = Fixture::start();
    let mut lease = GatewayLease::open(fixture.config(10), fixture.endpoint()).unwrap();
    assert!(lease.take_stream().is_none());
    authenticate(&fixture, &mut lease, TrustDecision::Once).unwrap();
    let mut client = lease.take_stream().unwrap();
    assert_eq!(lease.target().host, TARGET);
    assert!(lease.take_stream().is_none());
    client.set_read_timeout(Some(WAIT)).unwrap();
    client.set_write_timeout(Some(WAIT)).unwrap();
    assert!(client.peer_addr().unwrap().ip().is_loopback());
    assert!(TcpStream::connect_timeout(&client.peer_addr().unwrap(), WAIT).is_err());
    let mut server = fixture.accept_target();

    client.write_all(b"\xff\0target-request").unwrap();
    let mut request = [0; 16];
    server.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"\xff\0target-request");
    server.write_all(b"GATEWAY_OK").unwrap();
    let mut received_through_gateway = [0; 10];
    client.read_exact(&mut received_through_gateway).unwrap();
    assert_eq!(&received_through_gateway, b"GATEWAY_OK");
}

#[test]
fn cancelling_one_lease_releases_only_its_channel() {
    let fixture = Fixture::start();
    let mut first = GatewayLease::open(fixture.config(11), fixture.endpoint()).unwrap();
    authenticate(&fixture, &mut first, TrustDecision::Remember).unwrap();
    let mut first_target = fixture.accept_target();
    let mut first_stream = first.take_stream().unwrap();
    first_stream.set_read_timeout(Some(WAIT)).unwrap();

    let mut second = GatewayLease::open(fixture.config(12), fixture.endpoint()).unwrap();
    authenticate(&fixture, &mut second, TrustDecision::Once).unwrap();
    let mut second_target = fixture.accept_target();
    let mut second_stream = second.take_stream().unwrap();
    second_stream.set_read_timeout(Some(WAIT)).unwrap();
    assert_eq!(fixture.counts.active.load(SeqCst), 2);

    first.cancel();
    assert_eq!(first_stream.read(&mut [0]).unwrap(), 0);
    assert_eq!(first_target.read(&mut [0]).unwrap(), 0);
    let deadline = Instant::now() + WAIT;
    while fixture.counts.active.load(SeqCst) != 1 {
        pause(deadline);
    }
    second_target.write_all(b"live").unwrap();
    let mut received = [0; 4];
    second_stream.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"live");
    second_target.shutdown(Shutdown::Both).unwrap();
    drop(second_stream);
    while !second.is_finished() || fixture.counts.active.load(SeqCst) != 0 {
        pause(deadline);
    }
}

#[test]
fn target_failure_releases_gateway_channel() {
    let fixture = Fixture::start();
    let mut target = fixture.endpoint();
    target.host = "wrong-target.invalid".into();
    let mut lease = GatewayLease::open(fixture.config(13), target).unwrap();
    let error = authenticate(&fixture, &mut lease, TrustDecision::Once).unwrap_err();
    assert!(error.contains("Gateway could not open"), "{error}");
    assert_eq!(fixture.counts.attempted.load(SeqCst), 1);
    assert_eq!(fixture.counts.active.load(SeqCst), 0);
    assert!(lease.take_stream().is_none());
}

#[test]
fn changed_gateway_host_key_blocks_desktop() {
    let fixture = Fixture::start();
    let config = fixture.config(14);
    let old_key = fixture_key();
    std::fs::write(
        &config.app_known_hosts,
        format!(
            "[127.0.0.1]:{} {}",
            fixture.gateway_port,
            old_key.public_key().to_openssh().unwrap()
        ),
    )
    .unwrap();
    let mut lease = GatewayLease::open(config, fixture.endpoint()).unwrap();
    let deadline = Instant::now() + WAIT;
    let mut changed = false;
    loop {
        for event in lease.poll() {
            match event {
                ConnectionEvent::State { message, .. } => changed |= message.contains("changed"),
                ConnectionEvent::Exited { .. } => {
                    assert!(changed);
                    assert_eq!(fixture.counts.authenticated.load(SeqCst), 0);
                    assert_eq!(fixture.counts.attempted.load(SeqCst), 0);
                    assert!(lease.take_stream().is_none());
                    return;
                }
                ConnectionEvent::Connected { .. } | ConnectionEvent::Challenge { .. } => {
                    panic!("Changed gateway key must block forwarding and authentication");
                }
                _ => {}
            }
        }
        pause(deadline);
    }
}

#[test]
fn profile_gateway_keeps_target_credentials_and_identity_separate() {
    let session = Session {
        protocol: "RDP".into(),
        host: TARGET.into(),
        port: 3389,
        user: "desktop-account".into(),
        credential_ref: Some("target-reference".into()),
        gateway_credential_ref: Some("gateway-reference".into()),
        jump: "first@bastion:2222, gateway-user@[2001:db8::1]:2200".into(),
        ..Default::default()
    };
    let (gateway, endpoint) = harbor_workspace::ssh::desktop_gateway::profile(&session).unwrap();
    assert_eq!(
        endpoint,
        Endpoint {
            host: TARGET.into(),
            port: 3389
        }
    );
    assert_eq!(gateway.protocol, "SSH");
    assert_eq!(gateway.user, USER);
    assert_eq!(gateway.host, "2001:db8::1");
    assert_eq!(gateway.port, 2200);
    assert_eq!(gateway.jump, "first@bastion:2222");
    assert!(gateway.credential_ref.is_none());
    assert_eq!(
        gateway.gateway_credential_ref.as_deref(),
        Some("gateway-reference")
    );
    assert_eq!(session.user, "desktop-account");
    assert_eq!(session.credential_ref.as_deref(), Some("target-reference"));

    let (gateway, _) = harbor_workspace::ssh::desktop_gateway::profile(&Session {
        jump: "bastion".into(),
        ..session.clone()
    })
    .unwrap();
    assert!(
        gateway.user.is_empty(),
        "target user must not become gateway user"
    );
    assert!(
        harbor_workspace::ssh::desktop_gateway::profile(&Session {
            ssh_options: harbor_workspace::model::SshOptions {
                jump_limit: 1,
                ..session.ssh_options.clone()
            },
            ..session
        })
        .is_err()
    );
}

#[test]
fn vnc_frame_and_pointer_travel_through_the_gateway() {
    use harbor_workspace::desktop::{
        DesktopEvent, DesktopInput, DesktopSession, rdp_options::ClipboardPolicy,
    };

    let (listener, mut config) = vnc_fixture::endpoint(false, ClipboardPolicy::Disabled);
    let fixture = Fixture::with_target(listener.try_clone().unwrap());
    config.session.host = TARGET.into();
    config.session.jump = format!("{USER}@127.0.0.1:{}", fixture.gateway_port);

    let (gateway, endpoint) =
        harbor_workspace::ssh::desktop_gateway::profile(&config.session).unwrap();
    let mut gateway_config = fixture.config(15);
    gateway_config.session = gateway;
    gateway_config.session.agent_mode = AgentMode::Disabled;
    let mut lease = GatewayLease::open(gateway_config, endpoint).unwrap();
    authenticate(&fixture, &mut lease, TrustDecision::Once).unwrap();
    config.stream = lease.take_stream();

    let mut desktop = DesktopSession::connect(config).unwrap();
    let mut target = vnc_fixture::accept(listener);
    vnc_fixture::encodings(&mut target);
    vnc_fixture::read(&mut target, 10);
    vnc_fixture::update(&mut target, (0, 0, 1, 1), 0, &[3, 5, 7, 0]);
    vnc_fixture::read(&mut target, 10);

    let deadline = Instant::now() + WAIT;
    'frame: loop {
        for event in desktop.poll() {
            match event {
                DesktopEvent::Frame(frame) if frame.pixels[..4] == [7, 5, 3, 255] => break 'frame,
                DesktopEvent::Disconnected { message } => panic!("{message}"),
                _ => {}
            }
        }
        pause(deadline);
    }
    desktop
        .send(DesktopInput::Pointer {
            x: 11,
            y: 3,
            button: Some(1),
            pressed: true,
        })
        .unwrap();
    assert_eq!(vnc_fixture::read(&mut target, 6), [5, 1, 0, 11, 0, 3]);
    desktop.close();
    while fixture.counts.active.load(SeqCst) != 0 {
        pause(deadline);
    }
}

#[test]
fn rdp_retry_relay_reopens_the_channel_on_one_ssh_authentication() {
    use harbor_workspace::desktop::carrier::SocketRelay;

    let fixture = Fixture::start();
    let mut lease = GatewayLease::open_retryable(fixture.config(16), fixture.endpoint()).unwrap();
    authenticate(&fixture, &mut lease, TrustDecision::Once).unwrap();
    let retry = lease.retry_connector().unwrap();
    let relay = SocketRelay::for_rdp(lease.take_stream().unwrap(), Some(retry), WAIT).unwrap();
    let address = relay.address();

    let token = relay.token();
    let mut first_client = TcpStream::connect_timeout(&address, WAIT).unwrap();
    first_client.write_all(&token).unwrap();
    let mut first_target = fixture.accept_target();
    drop(first_client);
    assert_eq!(first_target.read(&mut [0]).unwrap(), 0);

    let mut invalid = TcpStream::connect_timeout(&address, WAIT).unwrap();
    invalid.set_read_timeout(Some(WAIT)).unwrap();
    let mut wrong = token;
    wrong[31] ^= 1;
    invalid.write_all(&wrong).unwrap();
    assert_eq!(invalid.read(&mut [0]).unwrap(), 0);
    assert_eq!(fixture.counts.attempted.load(SeqCst), 1);

    let mut second_client = TcpStream::connect_timeout(&address, WAIT).unwrap();
    second_client.set_read_timeout(Some(WAIT)).unwrap();
    second_client.write_all(&token).unwrap();
    let mut second_target = fixture.accept_target();
    second_target.write_all(b"RETRIED").unwrap();
    let mut received = [0; 7];
    second_client.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"RETRIED");
    assert_eq!(fixture.counts.attempted.load(SeqCst), 2);
    assert_eq!(fixture.counts.authenticated.load(SeqCst), 1);

    drop(lease);
    assert_eq!(second_client.read(&mut [0]).unwrap(), 0);
    let deadline = Instant::now() + WAIT;
    while !relay.is_finished() || fixture.counts.active.load(SeqCst) != 0 {
        pause(deadline);
    }
    assert!(TcpStream::connect_timeout(&address, WAIT).is_err());
}

#[test]
fn cancelled_retry_lease_closes_relay_before_helper_connects() {
    use harbor_workspace::desktop::carrier::SocketRelay;

    let fixture = Fixture::start();
    let mut lease = GatewayLease::open_retryable(fixture.config(17), fixture.endpoint()).unwrap();
    authenticate(&fixture, &mut lease, TrustDecision::Once).unwrap();
    let relay = SocketRelay::for_rdp(
        lease.take_stream().unwrap(),
        lease.retry_connector(),
        std::time::Duration::from_secs(60),
    )
    .unwrap();

    drop(lease);
    let deadline = Instant::now() + WAIT;
    while !relay.is_finished() {
        pause(deadline);
    }
    assert!(TcpStream::connect_timeout(&relay.address(), WAIT).is_err());
}

#[test]
fn jump_chain_verifies_and_authenticates_each_gateway() {
    let exit = Fixture::start();
    let entry = Fixture::with_jump_to(&exit);
    let mut config = exit.config(18);
    config.session.jump = format!("{USER}@127.0.0.1:{}", entry.gateway_port);
    let mut lease = GatewayLease::open(config, exit.endpoint()).unwrap();
    gateway_fixture::authenticate_chain(&[&entry, &exit], &mut lease, TrustDecision::Once).unwrap();

    let mut target_stream = exit.accept_target();
    let mut app_stream = lease.take_stream().unwrap();
    app_stream.set_read_timeout(Some(WAIT)).unwrap();
    target_stream.write_all(b"CHAIN_OK").unwrap();
    let mut received = [0; 8];
    app_stream.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"CHAIN_OK");
    assert_eq!(entry.counts.authenticated.load(SeqCst), 1);
    assert_eq!(exit.counts.authenticated.load(SeqCst), 1);
    assert_eq!(entry.counts.attempted.load(SeqCst), 1);
    assert_eq!(exit.counts.attempted.load(SeqCst), 1);
}

#[test]
fn saved_protocol_profile_owns_the_gateway_credential_binding() {
    use harbor_workspace::{
        credentials::CredentialBinding,
        ssh::{ChallengeAnswer, auth_profile_binding},
    };
    let fixture = Fixture::start();
    let session = Session {
        protocol: "VNC".into(),
        host: TARGET.into(),
        port: fixture.target_port,
        user: "vnc-auth-user".into(),
        jump: format!("{USER}@127.0.0.1:{}", fixture.gateway_port),
        agent_mode: AgentMode::Disabled,
        ..Default::default()
    };
    let mut lease = GatewayLease::open_for_session(fixture.config(19), &session, false).unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        for event in lease.poll() {
            match event {
                ConnectionEvent::Challenge {
                    id, host_key: true, ..
                } => lease
                    .answer(ChallengeAnswer {
                        id,
                        values: vec![],
                        trust: Some(TrustDecision::Once),
                    })
                    .unwrap(),
                ConnectionEvent::Challenge {
                    credential: Some(credential),
                    ..
                } => {
                    let CredentialBinding::Ssh {
                        profile,
                        host,
                        user,
                        port,
                        ..
                    } = credential.binding
                    else {
                        panic!("Expected SSH credential binding");
                    };
                    assert_eq!(profile, auth_profile_binding(&session));
                    assert_eq!(host, "127.0.0.1");
                    assert_eq!(port, fixture.gateway_port);
                    assert_eq!(user, USER);
                    return;
                }
                ConnectionEvent::Exited { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        pause(deadline);
    }
}
