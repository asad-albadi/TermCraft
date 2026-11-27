use super::gui_tests::{shell_frame, shell_text_position};
use super::terminal_review_tests::owned_workspace;
use super::*;

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn every_protocol_keeps_validation_and_actions_visible_at_scaled_viewports() {
    for scale in [1., 1.5, 2.] {
        let (ctx, _root, mut workspace) = owned_workspace();
        ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
        let size = vec2(1120., 760.) / scale;
        for protocol in PROTOCOLS {
            let host = "試験用ホスト名".repeat(30);
            workspace.session_form = Some(Session {
                protocol: (*protocol).into(),
                host: host.clone(),
                directory: "/長い名前/".repeat(35),
                jump: "test@bastion.invalid:22".into(),
                ..Default::default()
            });
            workspace.form_error = "Correct the connection settings.".into();
            let mut draw = || {
                shell_frame(&ctx, size, scale, vec![], |ui| {
                    workspace.session_dialog(ui.ctx())
                })
            };
            draw();
            let output = draw();
            for label in [
                "Correct the connection settings.",
                "Save only",
                "Cancel",
                "Connect",
            ] {
                assert!(
                    ctx.content_rect()
                        .contains(shell_text_position(&output, label)),
                    "{protocol} at {scale}"
                );
            }
            assert_eq!(workspace.session_form.as_ref().unwrap().host, host);
        }
    }
}

#[cfg(unix)]
#[test]
fn rename_field_owns_real_terminal_input_and_restores_the_invoker() {
    use super::terminal_review_tests::{captured, raw_terminal};
    let (ctx, root, mut workspace) = owned_workspace();
    let tab = raw_terminal(&mut workspace, "dialog-input-fixture");
    let terminal = Id::new(("terminal-input", tab));
    let size = vec2(1050., 720.);
    let draw = |w: &mut Workspace, events| {
        shell_frame(&ctx, size, 1., events, |ui| {
            w.set_shortcut_context(ui.ctx());
            w.terminals(ui);
            w.file_dialogs(ui.ctx());
        })
    };
    draw(&mut workspace, vec![]);
    ctx.memory_mut(|m| m.request_focus(terminal));
    let file = root.path().join("original.txt");
    std::fs::write(&file, "fixture").unwrap();
    workspace.rename_file = Some(file.clone());
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![Event::Text("draft".into())]);
    assert_eq!(workspace.rename_text, "draft");
    draw(&mut workspace, vec![key(Key::Escape)]);
    assert!(workspace.rename_file.is_none());
    draw(&mut workspace, vec![]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(terminal));
    draw(&mut workspace, vec![Event::Text("z".into())]);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        workspace.tabs[0].poll();
        let bytes = captured(&workspace.tabs[0]);
        if !bytes.is_empty() {
            assert_eq!(bytes, b"z");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "positive control did not reach the fixture"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(file.exists());
}

#[test]
fn nested_key_manager_escape_returns_to_the_session_draft() {
    let (ctx, _root, mut workspace) = owned_workspace();
    workspace.new_session("SSH");
    workspace.session_form.as_mut().unwrap().name = "Nested draft".into();
    let draw = |w: &mut Workspace, events| {
        shell_frame(&ctx, vec2(1150., 800.), 1., events, |ui| {
            w.session_dialog(ui.ctx());
            w.ssh_key_windows(ui.ctx());
        })
    };
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![]);
    let caller = ctx.memory(|memory| memory.focused()).unwrap();
    workspace.ssh_keys.open = true;
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![key(Key::Escape)]);
    assert!(!workspace.ssh_keys.open);
    assert_eq!(
        workspace.session_form.as_ref().unwrap().name,
        "Nested draft"
    );
    draw(&mut workspace, vec![]);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(caller));
}

#[test]
fn local_delete_enter_chooses_cancel_and_invalid_rename_keeps_the_draft() {
    use super::gui_tests::shell_pointer;
    let (ctx, root, mut workspace) = owned_workspace();
    let path = root.path().join("kept.txt");
    std::fs::write(&path, b"fixture").unwrap();
    let draw = |w: &mut Workspace, events| {
        shell_frame(&ctx, vec2(640., 450.), 1., events, |ui| {
            w.file_dialogs(ui.ctx())
        })
    };
    workspace.delete_file = Some(path.clone());
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![]);
    draw(&mut workspace, vec![key(Key::Enter)]);
    assert!(workspace.delete_file.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), b"fixture");
    workspace.rename_file = Some(path.clone());
    workspace.rename_text = "../invalid".into();
    draw(&mut workspace, vec![]);
    let output = draw(&mut workspace, vec![]);
    let rename = shell_text_position(&output, "Rename");
    draw(
        &mut workspace,
        vec![Event::PointerMoved(rename), shell_pointer(rename, true)],
    );
    draw(&mut workspace, vec![shell_pointer(rename, false)]);
    assert_eq!(workspace.rename_file.as_ref(), Some(&path));
    assert_eq!(workspace.rename_text, "../invalid");
    shell_text_position(&draw(&mut workspace, vec![]), "Enter a single file name.");
}

#[test]
fn local_dirty_exit_requires_a_decision_and_failed_save_keeps_the_document() {
    use super::gui_tests::shell_pointer;
    let (ctx, root, mut w) = owned_workspace();
    let path = root.path().join("absent").join("draft.txt");
    w.editor = Some(Editor {
        closing: false,
        closing_app: false,
        error: String::new(),
        path: path.clone(),
        text: "unsaved draft".into(),
        dirty: true,
    });
    let draw = |w: &mut Workspace, events| {
        shell_frame(&ctx, vec2(900., 660.), 1., events, |ui| {
            w.file_dialogs(ui.ctx())
        })
    };
    let mut native = RawInput::default();
    native
        .viewports
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = ctx.run_ui(native, |ui| w.intercept_session_close(ui.ctx()));
    output.textures_delta.clear();
    assert!(
        output.viewport_output[&ViewportId::ROOT]
            .commands
            .contains(&ViewportCommand::CancelClose)
    );
    draw(&mut w, vec![]);
    let save = shell_text_position(&draw(&mut w, vec![]), "Save and close");
    for pressed in [true, false] {
        draw(
            &mut w,
            vec![Event::PointerMoved(save), shell_pointer(save, pressed)],
        );
    }
    assert!(!w.editor.as_ref().unwrap().error.is_empty());
    assert_eq!(w.editor.as_ref().unwrap().text, "unsaved draft");
    draw(&mut w, vec![key(Key::Escape)]);
    assert!(!w.editor.as_ref().unwrap().closing_app);
    assert!(w.editor.as_ref().unwrap().dirty);
    assert!(!path.exists());
}

#[test]
fn real_session_rows_name_their_controls_and_allocate_readable_space() {
    use egui::accesskit::Role;
    for (width, scale) in [(980., 1.), (400., 1.), (400., 2.)] {
        let (ctx, _root, mut w) = owned_workspace();
        ctx.enable_accesskit();
        ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
        for protocol in PROTOCOLS {
            w.session_form = Some(Session {
                protocol: (*protocol).into(),
                ..Default::default()
            });
            w.session_form.as_mut().unwrap().browser_options.embedded = true;
            let mut draw = || {
                shell_frame(&ctx, vec2(width, 1600.), scale, vec![], |ui| {
                    w.session_dialog(ui.ctx())
                })
            };
            draw();
            let output = draw();
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            let mut labels = vec!["Session name", "Folder"];
            match *protocol {
                "Local" => labels.extend(["Shell executable", "Start folder"]),
                "Serial" => labels.extend(["Device / COM port", "Baud rate"]),
                "Browser" => labels.push("Website address"),
                _ => labels.extend(["Remote host", "Username", "Port"]),
            }
            if matches!(*protocol, "SSH" | "SFTP" | "Mosh") {
                labels.extend(["Private key", "Jump host"]);
            }
            if *protocol == "Browser" {
                labels.push("Proxy endpoint");
            }
            if *protocol == "RDP" {
                labels.push("Domain");
            }
            for label in labels {
                let (id, node) = nodes
                    .iter()
                    .find(|(_, n)| {
                        n.role() == Role::Label
                            && (n.value() == Some(label) || n.label() == Some(label))
                    })
                    .unwrap_or_else(|| panic!("missing {label} for {protocol}"));
                let bounds = node.bounds().unwrap();
                let field = nodes
                    .iter()
                    .find(|(_, n)| n.labelled_by().contains(id))
                    .unwrap_or_else(|| panic!("unlabelled {label} for {protocol}"))
                    .1
                    .bounds()
                    .unwrap();
                assert!(
                    bounds.y1 - bounds.y0 < 36.,
                    "{protocol}/{label}: {bounds:?}"
                );
                if !matches!(label, "Port" | "Baud rate") {
                    assert!(field.x1 - field.x0 >= 160., "{protocol}/{label}: {field:?}");
                }
                if width > 500. {
                    assert!(
                        field.x0 >= bounds.x1 - 1.,
                        "{protocol}/{label}: {bounds:?} {field:?}"
                    );
                } else {
                    assert!(
                        field.y0 >= bounds.y1 - 1.,
                        "{protocol}/{label}: {bounds:?} {field:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn key_manager_exposes_distinct_labels_for_both_masked_inputs() {
    let (ctx, _root, mut w) = owned_workspace();
    ctx.enable_accesskit();
    w.ssh_keys.open = true;
    let mut draw = |events| {
        shell_frame(&ctx, vec2(1000., 1500.), 1., events, |ui| {
            w.ssh_key_windows(ui.ctx())
        })
    };
    draw(vec![]);
    draw(vec![]);
    for (index, title) in [
        "Key passphrase (optional)",
        "Repeat passphrase for generation",
    ]
    .into_iter()
    .enumerate()
    {
        let id = Id::new(("key-manager", 0_u64, 0_u64, index));
        ctx.memory_mut(|m| m.request_focus(id));
        let nodes = draw(vec![Event::Text("sample".into())])
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes;
        let field = &nodes
            .iter()
            .find(|(node_id, _)| *node_id == id.accesskit_id())
            .unwrap()
            .1;
        assert!(nodes.iter().all(|(_, n)| {
            [n.label(), n.value()]
                .into_iter()
                .flatten()
                .all(|v| !v.contains("sample"))
        }));
        assert_eq!(field.role(), egui::accesskit::Role::PasswordInput);
        assert!(
            nodes.iter().any(|(id, n)| field.labelled_by().contains(id)
                && (n.label() == Some(title) || n.value() == Some(title))),
            "{title}"
        );
    }
}

#[cfg(unix)]
#[test]
fn modeless_tab_close_permits_input_to_an_unrelated_real_terminal() {
    use super::terminal_review_tests::{captured, raw_terminal};
    let (ctx, _root, mut w) = owned_workspace();
    let a = raw_terminal(&mut w, "document owner");
    let b = raw_terminal(&mut w, "independent terminal");
    w.pending_remote_tab_close = Some(a);
    let draw = |w: &mut Workspace, events| {
        shell_frame(&ctx, vec2(1100., 700.), 1., events, |ui| {
            w.set_shortcut_context(ui.ctx());
            w.terminals(ui);
        })
    };
    draw(&mut w, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new(("terminal-input", b))));
    draw(&mut w, vec![Event::Text("z".into())]);
    let deadline = Instant::now() + Duration::from_secs(2);
    let tab = w.tabs.iter_mut().find(|t| t.id == b).unwrap();
    loop {
        tab.poll();
        let bytes = captured(tab);
        if !bytes.is_empty() {
            assert_eq!(bytes, b"z");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "modeless review blocked the independent terminal"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
