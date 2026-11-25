use super::{TerminalTab, desktop, remote_files::AuthForm};
use harbor_workspace::{
    desktop::{DesktopConfig, DesktopSession},
    protocols::{LegacyProtocolConfig, Transport},
    ssh::{ChallengeAnswer, ConnectionConfig, ConnectionEvent, desktop_gateway::GatewayLease},
};

pub(super) struct Pending {
    lease: GatewayLease,
    target: Option<Target>,
}

enum Target {
    Desktop(Box<DesktopConfig>),
    Telnet(LegacyProtocolConfig),
}

impl Pending {
    pub(super) fn answer(&self, answer: ChallengeAnswer) -> Result<(), String> {
        self.lease.answer(answer)
    }

    pub(super) fn is_finished(&self) -> bool {
        self.lease.is_finished()
    }
}

pub(super) fn start(
    tab: &mut TerminalTab,
    config: ConnectionConfig,
    target: DesktopConfig,
) -> Result<(), String> {
    start_target(tab, config, Target::Desktop(Box::new(target)))
}

pub(super) fn start_telnet(tab: &mut TerminalTab, config: ConnectionConfig) -> Result<(), String> {
    let target = LegacyProtocolConfig::from_session(
        &tab.session,
        tab.screen.cols as u16,
        tab.screen.rows as u16,
    )?;
    start_target(tab, config, Target::Telnet(target))
}

fn start_target(
    tab: &mut TerminalTab,
    config: ConnectionConfig,
    target: Target,
) -> Result<(), String> {
    let retryable =
        matches!(&target, Target::Desktop(desktop) if desktop.session.protocol == "RDP");
    let lease = GatewayLease::open_for_session(config, &tab.session, retryable)?;
    tab.gateway = Some(Pending {
        lease,
        target: Some(target),
    });
    tab.connection_status = "Connecting to SSH gateway…".into();
    Ok(())
}

pub(super) fn poll(tab: &mut TerminalTab) {
    let Some(mut pending) = tab.gateway.take() else {
        return;
    };
    if tab
        .desktop
        .as_ref()
        .is_some_and(|desktop| desktop.handle.is_finished())
        || tab.legacy.as_ref().is_some_and(Transport::is_finished)
    {
        return;
    }

    for event in pending.lease.poll() {
        match event {
            ConnectionEvent::State {
                connection,
                message,
            } if connection == tab.id => tab.connection_status = message,
            ConnectionEvent::Challenge {
                connection,
                id,
                title,
                prompts,
                host_key,
                credential,
            } if connection == tab.id => {
                tab.auth = Some(AuthForm {
                    id,
                    title,
                    values: (0..prompts.len()).map(|_| Default::default()).collect(),
                    prompts,
                    host_key,
                    credential,
                    save_in_vault: false,
                    vault_filled: false,
                });
            }
            ConnectionEvent::Connected { connection } if connection == tab.id => {
                tab.auth = None;
                let Some(target) = pending.target.take() else {
                    continue;
                };
                let Some(stream) = pending.lease.take_stream() else {
                    failed(
                        tab,
                        "The SSH gateway stream closed before it could be used.".into(),
                    );
                    return;
                };
                if let Err(error) = connect_target(tab, &pending, target, stream) {
                    failed(tab, error);
                    return;
                }
            }
            ConnectionEvent::Exited {
                connection,
                message,
            } if connection == tab.id => {
                tab.auth = None;
                if pending.target.is_some() || message != "Connection ended" {
                    if let Some(legacy) = tab.legacy.as_mut() {
                        legacy.close();
                    }
                    failed(tab, message);
                }
                return;
            }
            _ => {}
        }
    }
    tab.gateway = Some(pending);
}

fn connect_target(
    tab: &mut TerminalTab,
    pending: &Pending,
    target: Target,
    stream: std::net::TcpStream,
) -> Result<(), String> {
    match target {
        Target::Desktop(mut config) => {
            config.stream = Some(stream);
            config.gateway_retry = pending.lease.retry_connector();
            let handle = DesktopSession::connect(*config)?;
            tab.desktop = Some(desktop::State::new(handle, &tab.session));
        }
        Target::Telnet(mut config) => {
            config.carrier = Some(stream);
            config.cols = tab.screen.cols as u16;
            config.rows = tab.screen.rows as u16;
            tab.legacy = Some(Transport::connect(config)?);
            tab.connection_status = "Telnet connected through SSH gateway".into();
        }
    }
    tab.error.clear();
    Ok(())
}

fn failed(tab: &mut TerminalTab, message: String) {
    if tab.legacy.is_none() && tab.desktop.is_none() {
        tab.output_ended = true;
        tab.input_failed = true;
    }
    tab.connection_status = "SSH gateway connection ended".into();
    tab.error = message;
}

#[cfg(all(test, unix))]
#[allow(dead_code)]
#[path = "../../tests/support/desktop_gateway_server.rs"]
mod gateway_fixture;

#[cfg(all(test, unix))]
#[allow(dead_code)]
#[path = "../../tests/support/rdp_server.rs"]
mod rdp_fixture;

#[cfg(all(test, unix))]
mod tests {
    use super::{
        gateway_fixture::{Fixture, TARGET, USER, WAIT, pause},
        *,
    };
    use crate::ui::terminal_review_tests::owned_workspace;
    use harbor_workspace::{
        model::{AgentMode, Session},
        ssh::TrustDecision,
    };
    use std::{
        io::{Read, Write},
        sync::atomic::Ordering::SeqCst,
        time::Instant,
    };

    #[test]
    fn telnet_tab_signs_in_to_gateway_and_closes_its_target() {
        let fixture = Fixture::start();
        let (_ctx, _root, mut workspace) = owned_workspace();
        workspace.start(Session {
            protocol: "Telnet".into(),
            host: TARGET.into(),
            port: fixture.target_port,
            user: "remote-telnet-user".into(),
            jump: format!("{USER}@127.0.0.1:{}", fixture.gateway_port),
            agent_mode: AgentMode::Disabled,
            ..Default::default()
        });
        let tab = &mut workspace.tabs[0];
        let deadline = Instant::now() + WAIT;
        let mut host_key_verified = false;
        let mut authenticated = false;
        while tab.legacy.is_none() {
            tab.poll();
            if let Some(form) = tab.auth.as_mut() {
                if form.host_key {
                    assert!(form.title.contains(&fixture.host_key));
                    host_key_verified = true;
                    tab.answer_auth(Some(TrustDecision::Once));
                } else {
                    assert!(host_key_verified);
                    assert!(form.title.contains(USER));
                    form.values[0].push_str("fixture-gateway-secret");
                    authenticated = true;
                    tab.answer_auth(None);
                }
            }
            assert!(tab.error.is_empty(), "{}", tab.error);
            if tab.legacy.is_none() {
                pause(deadline);
            }
        }
        assert!(authenticated);
        assert_eq!(tab.session.host, TARGET);
        assert_eq!(tab.session.user, "remote-telnet-user");
        let mut target = fixture.accept_target();
        target.write_all(&[255, 253, 39]).unwrap();
        let mut negotiation = [0; 3];
        target.read_exact(&mut negotiation).unwrap();
        assert_eq!(negotiation, [255, 251, 39]);
        target.write_all(&[255, 250, 39, 1, 255, 240]).unwrap();
        let mut expected = vec![255, 250, 39, 0, 0];
        expected.extend_from_slice(b"USER\x01remote-telnet-user");
        expected.extend_from_slice(&[255, 240]);
        let mut reported_user = vec![0; expected.len()];
        target.read_exact(&mut reported_user).unwrap();
        assert_eq!(reported_user, expected);
        target.write_all(b"GATEWAY_TELNET_OK\r\n").unwrap();
        loop {
            tab.poll();
            let rows = (0..tab.screen.rows)
                .map(|row| {
                    (0..tab.screen.cols)
                        .filter_map(|col| tab.screen.cell(row, col))
                        .map(|cell| cell.text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            if rows.iter().any(|line| line.contains("GATEWAY_TELNET_OK")) {
                break;
            }
            assert!(tab.error.is_empty(), "{}", tab.error);
            pause(deadline);
        }
        workspace.force_close_tab(0);
        let mut buffer = [0; 256];
        while target.read(&mut buffer).unwrap() != 0 {}
        while fixture.counts.active.load(SeqCst) != 0 {
            pause(deadline);
        }
    }

    #[test]
    fn rdp_tab_retries_after_certificate_trust_with_original_nla_identity() {
        use harbor_workspace::desktop::{DesktopInput, TrustDecision as DesktopTrust};
        let rdp = super::rdp_fixture::Server::start();
        let fixture = Fixture::with_external_target(rdp.address.port());
        let (_ctx, _root, mut workspace) = owned_workspace();
        let mut session = rdp.config(8012).session;
        session.host = TARGET.into();
        session.jump = format!("{USER}@127.0.0.1:{}", fixture.gateway_port);
        session.agent_mode = AgentMode::Disabled;
        workspace.start(session);

        let tab = &mut workspace.tabs[0];
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        let mut trusted_gateway = false;
        let mut trusted_rdp_cert = false;
        let mut submitted_rdp_password = false;
        loop {
            tab.poll();
            assert!(tab.error.is_empty(), "{}", tab.error);
            if let Some(form) = tab.auth.as_mut() {
                if form.host_key {
                    assert!(form.title.contains(&fixture.host_key));
                    tab.answer_auth(Some(TrustDecision::Once));
                    trusted_gateway = true;
                } else {
                    assert!(trusted_gateway);
                    assert!(form.title.contains(USER));
                    form.values[0].push_str("fixture-gateway-secret");
                    tab.answer_auth(None);
                }
            }
            if let Some(desktop) = &mut tab.desktop {
                match desktop.form.take() {
                    Some(desktop::Form::Certificate { id, .. }) => {
                        assert!(trusted_gateway);
                        assert_eq!(fixture.counts.attempted.load(SeqCst), 1);
                        desktop
                            .handle
                            .send(DesktopInput::Trust {
                                id,
                                decision: DesktopTrust::Once,
                            })
                            .unwrap();
                        trusted_rdp_cert = true;
                    }
                    Some(desktop::Form::Credentials {
                        id,
                        host,
                        port,
                        user,
                        fingerprint,
                        ..
                    }) => {
                        assert!(trusted_rdp_cert);
                        assert_eq!(host, TARGET);
                        assert_eq!(port, rdp.address.port());
                        assert_eq!(user, "fixture-user");
                        assert!(!fingerprint.is_empty());
                        assert_eq!(fixture.counts.attempted.load(SeqCst), 2);
                        desktop
                            .handle
                            .send(DesktopInput::Authenticate {
                                id,
                                user,
                                password: harbor_workspace::credentials::Secret::new(
                                    "fixture-password",
                                ),
                                domain: String::new(),
                            })
                            .unwrap();
                        submitted_rdp_password = true;
                    }
                    None => {}
                }
                if desktop
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.pixels.get(..4) == Some(&[255, 0, 0, 255]))
                {
                    assert!(desktop.connected);
                    break;
                }
            }
            pause(deadline);
        }
        assert!(submitted_rdp_password);
        assert_eq!(fixture.counts.authenticated.load(SeqCst), 1);
        workspace.force_close_tab(0);
        while fixture.counts.active.load(SeqCst) != 0 {
            pause(deadline);
        }
    }
}
