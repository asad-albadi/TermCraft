use super::*;
use crate::ui::terminal_review_tests::{owned_process_fixture_slot, owned_workspace};
use harbor_workspace::credentials::{agent::AgentTarget, keys::KeySettings};
#[allow(dead_code)]
#[path = "../../tests/support/ssh_server.rs"]
mod ssh_server;
fn wait(ctx: &Context, w: &mut Workspace) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        w.poll_ssh_keys(ctx);
        if !w.ssh_keys_busy() {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn owned_agent_selected_in_the_gui_authenticates_terminal_and_shared_source() {
    let _process_slot = owned_process_fixture_slot();
    let (ctx, root, mut w) = owned_workspace();
    w.start_owned_agent(&ctx);
    wait(&ctx, &mut w);
    assert!(w.owned_agent_endpoint().is_some());
    let server = ssh_server::SshServer::start(true);
    let config = server.config(1, true);
    fs::create_dir_all(root.path().join("ssh")).unwrap();
    fs::copy(&config.app_known_hosts, root.path().join("ssh/known_hosts")).unwrap();
    w.add_agent_identity(
        AgentTarget::Owned,
        PathBuf::from(&config.session.key),
        None,
        None,
        &ctx,
    );
    wait(&ctx, &mut w);
    let mut session = config.session;
    session.key.clear();
    session.agent_mode = AgentMode::Owned;
    let session_id = session.id.clone();
    w.store.sessions.push(session.clone());
    w.persist();
    w.start(session);
    let end = Instant::now() + Duration::from_secs(5);
    while !w.tabs[0].ssh_connected {
        w.tabs[0].poll();
        assert!(
            w.tabs[0].auth.is_none(),
            "Unexpected terminal authentication prompt"
        );
        assert!(
            w.tabs[0].ssh_connected || Instant::now() < end,
            "Terminal did not connect: {}; SSH public-key authentications: {}",
            w.tabs[0].error,
            server.accepted_authentications()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(server.accepted_authentications(), 1);
    let mut shared = Store::default();
    shared.sessions.push(Session {
        name: "from source".into(),
        host: "shared.test".into(),
        ..Default::default()
    });
    let path = server.root.join("source.json");
    shared.save(&path).unwrap();
    let cfg = harbor_workspace::sessions::shared::SharedSourceConfig::new(
        "Team",
        harbor_workspace::sessions::shared::SharedLocation::Sftp {
            session_id,
            path: path.to_str().unwrap().into(),
        },
        harbor_workspace::sessions::shared::SharedFormat::Workspace,
    )
    .unwrap();
    let id = cfg.id.clone();
    w.add_shared_source(cfg, &ctx);
    let end = Instant::now() + Duration::from_secs(6);
    while w.shared_snapshot(&id).is_none() {
        w.poll_shared_sources(&ctx);
        let progress = || {
            format!(
                "Shared source: {:?}; SSH public-key authentications: {}",
                w.shared_source_progress(&id),
                server.accepted_authentications()
            )
        };
        assert!(
            !w.has_shared_prompt(),
            "Unexpected authentication prompt. {}",
            progress()
        );
        assert!(
            w.shared_snapshot(&id).is_some() || (w.shared_sources_busy() && Instant::now() < end),
            "No shared source snapshot. {}",
            progress()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(server.accepted_authentications(), 2);
    assert_eq!(
        w.shared_snapshot(&id).unwrap().entries[0].session.name,
        "from source"
    );
}
#[test]
fn background_key_generation_records_the_real_created_paths() {
    let (ctx, root, mut w) = owned_workspace();
    let destination = root.path().join("gui generated key");
    w.generate_key_bundle(destination.clone(), KeySettings::default(), &ctx);
    wait(&ctx, &mut w);
    let generated = w.generated_key_paths().unwrap();
    assert!(generated.private.is_file());
    assert!(generated.public.is_file());
    assert!(generated.private.starts_with(destination));
}
#[test]
fn key_manager_footer_is_visible_at_the_minimum_app_height() {
    let (ctx, _root, mut w) = owned_workspace();
    w.ssh_keys.open = true;
    fn position(shape: &egui::epaint::Shape) -> Option<(f32, f32)> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Close" => {
                Some((t.pos.y, t.pos.y + t.galley.size().y))
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(position),
            _ => None,
        }
    }
    let mut close = None;
    for _ in 0..3 {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(920., 620.))),
                ..Default::default()
            },
            |ui| w.ssh_key_windows(ui.ctx()),
        );
        output.textures_delta.clear();
        close = output.shapes.iter().find_map(|s| position(&s.shape));
    }
    let (top, bottom) = close.expect("Visible Close control");
    assert!(
        top >= 0. && bottom <= 620.,
        "Close control is outside the app viewport: {top}..{bottom}"
    );
}
