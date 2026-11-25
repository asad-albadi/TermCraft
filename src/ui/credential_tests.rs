use super::*;
use crate::ui::terminal_review_tests::owned_workspace;
use harbor_workspace::credentials::Secret;
#[allow(dead_code)]
#[path = "../../tests/support/auth_server.rs"]
mod auth_server;
#[allow(dead_code)]
#[path = "../../tests/support/ssh_server.rs"]
mod ssh_server;
fn wait_vault(ctx: &Context, w: &mut Workspace) {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        w.poll_vault(ctx);
        if !w.vault_busy() {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn wait_auth(w: &mut Workspace, index: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    while w.tabs[index].auth.is_none() {
        w.tabs[index].poll();
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn optional_saved_password_stays_encrypted_and_requires_a_matching_live_challenge() {
    let (ctx, root, mut w) = owned_workspace();
    w.create_vault(Secret::new("owned master password"), &ctx);
    wait_vault(&ctx, &mut w);
    assert!(w.vault_unlocked());
    let server = auth_server::AuthServer::start(auth_server::Mode::Password);
    let config = server.config();
    let session = config.session.clone();
    fs::create_dir_all(root.path().join("ssh")).unwrap();
    fs::copy(config.app_known_hosts, root.path().join("ssh/known_hosts")).unwrap();
    w.store.sessions.push(session.clone());
    w.persist();
    w.start(session);
    wait_auth(&mut w, 0);
    let auth = w.tabs[0].auth.as_mut().unwrap();
    assert!(!auth.save_in_vault);
    assert!(auth.credential.is_some());
    auth.values[0] = zeroize::Zeroizing::new("fixture-password".into());
    let old_challenge = auth.id;
    auth.save_in_vault = true;
    w.tabs[0].answer_auth(None);
    wait_vault(&ctx, &mut w);
    let reference = w.store.sessions[0]
        .credential_ref
        .clone()
        .expect("Explicit save links an opaque reference");
    assert!(
        !fs::read_to_string(&w.store_path)
            .unwrap()
            .contains("fixture-password")
    );
    assert!(
        !fs::read(w.vault_path())
            .unwrap()
            .windows(b"fixture-password".len())
            .any(|v| v == b"fixture-password")
    );
    w.start(w.store.sessions[0].clone());
    wait_auth(&mut w, 1);
    let auth = w.tabs[1].auth.as_ref().unwrap();
    assert_eq!(
        auth.credential.as_ref().unwrap().reference.as_deref(),
        Some(reference.as_str())
    );
    let current_challenge = auth.id;
    let credential = auth.credential.clone().unwrap();
    w.request_vault(
        credentials::Request::use_terminal(w.tabs[1].id, old_challenge, credential.clone()),
        &ctx,
    );
    assert!(
        w.tabs[1].auth.as_ref().unwrap().values[0].is_empty(),
        "A retired challenge cannot fill its replacement"
    );
    let request = credentials::Request::use_terminal(w.tabs[1].id, current_challenge, credential);
    w.request_vault(request, &ctx);
    w.poll_vault(&ctx);
    assert_eq!(
        w.tabs[1].auth.as_ref().unwrap().values[0].as_str(),
        "fixture-password"
    );
    assert!(
        w.tabs[1].auth.is_some(),
        "Saved value is filled, never submitted automatically"
    );
    w.delete_vault_credential(reference.clone(), &ctx);
    wait_vault(&ctx, &mut w);
    assert!(w.store.sessions[0].credential_ref.is_none());
    assert!(
        w.tabs[1].auth.as_ref().unwrap().values[0].is_empty(),
        "Deletion clears copies filled from that entry"
    );
    w.lock_vault();
    assert!(!w.vault_unlocked());
    assert!(w.tabs[1].auth.as_ref().unwrap().values[0].is_empty());
}
#[allow(dead_code)]
#[path = "../../tests/support/ssh_proxy.rs"]
mod ssh_proxy;
#[test]
fn explicit_proxy_save_links_only_proxy_reference_and_encrypts_both_fields() {
    let (ctx, root, mut w) = owned_workspace();
    w.create_vault(Secret::new("proxy vault master"), &ctx);
    wait_vault(&ctx, &mut w);
    let server = ssh_server::SshServer::start(true);
    let proxy =
        ssh_proxy::Proxy::http_authenticated(format!("127.0.0.1:{}", server.port).parse().unwrap());
    let cfg = server.config(7040, true);
    fs::create_dir_all(root.path().join("ssh")).unwrap();
    fs::copy(cfg.app_known_hosts, root.path().join("ssh/known_hosts")).unwrap();
    let mut session = cfg.session;
    session.ssh_options.proxy = Some(harbor_workspace::model::ProxySettings {
        kind: Default::default(),
        host: "127.0.0.1".into(),
        port: proxy.address.port(),
        credential_ref: None,
        authenticate: true,
    });
    w.store.sessions.push(session.clone());
    w.start(session);
    wait_auth(&mut w, 0);
    let auth = w.tabs[0].auth.as_ref().unwrap();
    assert_eq!(auth.values.len(), 2);
    let credential = auth.credential.clone().unwrap();
    w.request_vault(
        credentials::Request::Save {
            destination: credentials::Destination::Terminal {
                connection: w.tabs[0].id,
                challenge: auth.id,
            },
            credential,
            values: vec![
                zeroize::Zeroizing::new("proxy-user".into()),
                zeroize::Zeroizing::new("SENTINEL_PROXY_PASSWORD".into()),
            ],
        },
        &ctx,
    );
    wait_vault(&ctx, &mut w);
    let reference = w.store.sessions[0]
        .ssh_options
        .proxy
        .as_ref()
        .unwrap()
        .credential_ref
        .clone()
        .expect("Proxy reference linked");
    assert!(w.store.sessions[0].credential_ref.is_none());
    assert!(w.store.sessions[0].gateway_credential_refs.is_empty());
    for path in [&w.store_path, &w.vault_path()] {
        let data = fs::read(path).unwrap();
        assert!(
            !data
                .windows(b"SENTINEL_PROXY_PASSWORD".len())
                .any(|v| v == b"SENTINEL_PROXY_PASSWORD")
        );
    }
    w.delete_vault_credential(reference, &ctx);
    wait_vault(&ctx, &mut w);
    assert!(
        w.store.sessions[0]
            .ssh_options
            .proxy
            .as_ref()
            .unwrap()
            .credential_ref
            .is_none()
    );
    w.tabs[0].ssh.as_mut().unwrap().cancel();
}
