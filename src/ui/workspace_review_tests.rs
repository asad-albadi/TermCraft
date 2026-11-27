use super::terminal_review_tests::{captured, frame, owned_workspace, raw_terminal};
use super::*;
#[cfg(unix)]
#[test]
fn opaque_client_password_keyboard_and_paste_stay_private() {
    let (ctx, _root, mut w) = owned_workspace();
    let peer = raw_terminal(&mut w, "unrelated shell");
    w.start_with_launch(Session{protocol:"SSH".into(),ssh_engine:SshEngine::OpenSsh,name:"compatibility client".into(),..Default::default()},Some(connection::Launch{rdp_profile:None,program:"python3".into(),args:vec!["-I".into(),"-c".into(),"import os,tty\ntty.setraw(0)\nos.write(1,b'Password: ')\nwhile True:\n data=os.read(0,32768)\n if not data: break\n os.write(1,b'\\r\\nCAP:'+data.hex().encode()+b'\\r\\n')".into()],directory:None}));
    let client = w.tabs[1].id;
    let deadline = Instant::now() + Duration::from_secs(3);
    super::terminal_review_tests::wait_for_fixture(&mut w.tabs[1], deadline, "Password:", |tab| {
        tab.screen.plain().contains("Password:")
    });
    w.enable_broadcast_all();
    assert!(
        !w.broadcast_group.selected().contains(&client),
        "client authentication is not observable and must not be auto-enrolled"
    );
    // Defensive validation must hold even with stale/forged membership.
    w.broadcast_group.select(client, true);
    w.broadcast_group.select(peer, true);
    frame(&ctx, &mut w, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", client))));
    frame(&ctx, &mut w, vec![Event::Text("secret".into())]);
    w.queue_paste(client, "private paste".into());
    w.resolve_paste(true);
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured(&w.tabs[1]).len() < 19 && Instant::now() < deadline {
        for t in &mut w.tabs {
            t.tick_paste(Instant::now());
            t.poll();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(captured(&w.tabs[1]), b"secretprivate paste");
    assert!(
        captured(&w.tabs[0]).is_empty(),
        "password reached an unrelated transport"
    );
    assert_eq!(
        w.paste_targets(peer),
        vec![peer],
        "private clients cannot be broadcast recipients either"
    );
}
#[test]
fn multiexec_toggle_preserves_explicit_membership() {
    let (_ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "A");
    let b = raw_terminal(&mut w, "B");
    w.enable_broadcast_all();
    w.broadcast_group.select(b, false);
    w.set_broadcast_enabled(false);
    w.set_broadcast_enabled(true);
    assert_eq!(w.broadcast_group.selected(), vec![a]);
    let c = raw_terminal(&mut w, "C");
    assert!(!w.broadcast_group.selected().contains(&c));
}
#[test]
fn multiexec_failure_survives_next_success_until_acknowledged() {
    let (_ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "A");
    let b = raw_terminal(&mut w, "B");
    w.enable_broadcast_all();
    let process = w.tabs[1].process.take();
    w.dispatch_keyboard(a, b"x");
    assert_eq!(w.broadcast_report.failed[0].0, b);
    w.tabs[1].process = process;
    w.dispatch_keyboard(a, b"y");
    assert_eq!(
        w.broadcast_report.failed[0].0, b,
        "later success hid a divergence"
    );
    w.acknowledge_broadcast_failures();
    assert!(w.broadcast_report.failed.is_empty());
}
#[test]
fn embedded_child_cannot_close_a_different_focused_window() {
    let (ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "A");
    let b = raw_terminal(&mut w, "B");
    w.detach_terminal(b);
    let child = w.windows.window(b);
    w.store.preferences.confirm_session_close = false;
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", a))));
    let mut output = ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
        |ui| {
            w.handle_shortcuts(ui.ctx(), child);
            w.handle_shortcuts(ui.ctx(), 0);
        },
    );
    output.textures_delta.clear();
    assert_eq!(w.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b]);
}
#[test]
fn detached_dispatcher_consumes_modifier_text_before_painting_terminal() {
    use harbor_workspace::workspace::shortcuts::{Action, KeyChord};
    let (ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "child");
    w.detach_terminal(id);
    let child = w.windows.window(id);
    w.store
        .preferences
        .shortcuts
        .bind(
            Action::ToggleSidebar,
            KeyChord::new("E", false, false, true),
        )
        .unwrap();
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
    let prefs = w.store.preferences.clone();
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 800.))),
            events: vec![
                Event::Key {
                    key: Key::E,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                },
                Event::Text("€".into()),
            ],
            ..Default::default()
        },
        |ui| {
            w.handle_shortcuts(ui.ctx(), child);
            super::pages::terminal_view(ui, &mut w.tabs[0], &prefs);
        },
    );
    output.textures_delta.clear();
    std::thread::sleep(Duration::from_millis(40));
    w.tabs[0].poll();
    assert!(captured(&w.tabs[0]).is_empty());
}
#[test]
fn detach_reconciles_root_toolbar_and_reconnect_preserves_window_and_peers() {
    let (_ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "root A");
    let b = raw_terminal(&mut w, "child B");
    w.detach_terminal(b);
    let window = w.windows.window(b);
    assert_eq!(
        w.tabs[w.active].id, a,
        "root toolbar must name the attached pane it shows"
    );
    w.windows.rename(b, "custom B".into());
    w.windows.enter_fullscreen(window, None);
    let c = raw_terminal(&mut w, "child C");
    w.detach_terminal(c);
    w.windows.reattach(c, window).unwrap();
    let root_before = w.layout_state.root.as_ref().unwrap().ids();
    let index = w.tabs.iter().position(|t| t.id == b).unwrap();
    w.reconnect_tab_now(index);
    let replacement = w
        .tabs
        .iter()
        .find(|t| t.session.name == "child B")
        .unwrap()
        .id;
    assert_eq!(w.windows.window(replacement), window);
    assert_eq!(w.windows.window(c), window);
    assert!(w.windows.fullscreen(window));
    assert_eq!(w.terminal_title(replacement), "custom B");
    assert_eq!(w.layout_state.root.as_ref().unwrap().ids(), root_before);
    assert_eq!(w.tabs[w.active].id, a);
}
fn child_frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>) {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 850.))),
            events,
            ..Default::default()
        },
        |ui| w.detached_windows(ui.ctx()),
    );
    output.textures_delta.clear();
}
#[test]
fn detached_find_query_is_edited_in_child_instead_of_sent_to_shell() {
    let (ctx, _root, mut w) = owned_workspace();
    let id = raw_terminal(&mut w, "child");
    w.detach_terminal(id);
    child_frame(&ctx, &mut w, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", id))));
    child_frame(
        &ctx,
        &mut w,
        vec![Event::Key {
            key: Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        }],
    );
    child_frame(&ctx, &mut w, vec![]);
    child_frame(&ctx, &mut w, vec![Event::Text("needle".into())]);
    assert_eq!(w.tabs[0].search, "needle");
    std::thread::sleep(Duration::from_millis(40));
    w.tabs[0].poll();
    assert!(captured(&w.tabs[0]).is_empty());
}
#[test]
fn output_after_last_paint_stays_unread_when_switching_to_another_tab() {
    let (ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "A");
    let b = raw_terminal(&mut w, "B");
    w.active = 0;
    w.set_layout_preset(harbor_workspace::workspace::layout::LayoutPreset::Single);
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1300., 900.))),
            ..Default::default()
        },
        |ui| w.render(ui),
    );
    output.textures_delta.clear();
    w.activity.mark_seen(a);
    w.tabs[0].send(b"late output");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !w.tabs[0].output_changed && Instant::now() < deadline {
        w.tabs[0].poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(w.tabs[0].output_changed);
    w.pump_terminals();
    w.active = 1;
    w.layout_state.focus(b);
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1300., 900.))),
            ..Default::default()
        },
        |ui| w.render(ui),
    );
    output.textures_delta.clear();
    assert!(
        w.activity.unread(a),
        "the newly changed A revision was never painted"
    );
}
#[test]
fn tiny_output_on_four_large_screens_has_bounded_activity_overhead() {
    let mut screens: Vec<_> = (0..4).map(|_| Screen::new(500, 200, 100)).collect();
    let started = Instant::now();
    for screen in &mut screens {
        let before = screen.activity_signature();
        screen.feed(b"x");
        assert_ne!(before, screen.activity_signature());
    }
    let elapsed = started.elapsed();
    eprintln!("four 500x200 screens, one byte each, activity accounting: {elapsed:?}");
    assert!(
        elapsed < Duration::from_millis(100),
        "activity accounting scanned whole grids: {elapsed:?}"
    );
}
fn confirmation_text(ctx: &Context, w: &mut Workspace) -> String {
    fn text(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for s in shapes {
                    text(s, out);
                }
            }
            _ => {}
        }
    }
    let mut result = String::new();
    for _ in 0..2 {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 800.))),
                ..Default::default()
            },
            |ui| w.session_confirmations(ui.ctx()),
        );
        output.textures_delta.clear();
        result.clear();
        for shape in output.shapes {
            text(&shape.shape, &mut result);
        }
    }
    result
}
#[test]
fn paste_and_close_confirmations_show_custom_titles_and_terminal_identity() {
    let (ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "same session");
    let b = raw_terminal(&mut w, "same session");
    w.windows.rename(a, "Staging".into());
    w.windows.rename(b, "Production".into());
    w.enable_broadcast_all();
    w.queue_paste(a, "one\ntwo".into());
    let paste = confirmation_text(&ctx, &mut w);
    assert!(
        paste.contains("Staging") && paste.contains("Production"),
        "{paste}"
    );
    assert!(paste.contains(&format!("#{a}")) && paste.contains(&format!("#{b}")));
    w.resolve_paste(false);
    w.request_window_session_close(&ctx);
    let close = confirmation_text(&ctx, &mut w);
    assert!(
        close.contains("Staging") && close.contains("Production"),
        "{close}"
    );
    assert!(close.contains(&format!("#{a}")) && close.contains(&format!("#{b}")));
}
