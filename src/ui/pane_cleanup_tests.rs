use super::*;

pub(super) fn labels(output: &egui::FullOutput) -> String {
    fn text(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::epaint::Shape::Vec(v) => {
                for shape in v {
                    text(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for shape in &output.shapes {
        text(&shape.shape, &mut out);
    }
    out
}

#[test]
fn tools_default_prioritizes_the_selected_task() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    let draw = |w: &mut Workspace| {
        gui_tests::shell_frame(&ctx, vec2(1360., 1800.), 1., vec![], |ui| w.tools_page(ui))
    };
    draw(&mut workspace);
    let output = draw(&mut workspace);
    let host = gui_tests::shell_text_position(&output, "Host");
    assert!(
        host.y < 220.,
        "Network task buried beneath unrelated tools: {host:?}"
    );
    let text = labels(&output);
    assert!(
        !text.contains("Portable Unix"),
        "Package workflows crowd the network task"
    );
}

#[test]
fn server_page_shows_one_configuration_at_a_time() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    let draw = |w: &mut Workspace| {
        gui_tests::shell_frame(&ctx, vec2(1360., 1800.), 1., vec![], |ui| {
            w.servers_page(ui)
        })
    };
    draw(&mut workspace);
    let output = draw(&mut workspace);
    let text = labels(&output);
    assert!(text.contains("HTTP file server"));
    assert!(
        !text.contains("FTP file server"),
        "Multiple unrelated service forms are expanded: {text}"
    );
}

#[test]
fn service_chooser_switches_views_without_losing_http_draft() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    workspace.http_root = "/owned/draft-folder".into();
    let draw = |w: &mut Workspace, events| {
        gui_tests::shell_frame(&ctx, vec2(980., 760.), 1., events, |ui| w.servers_page(ui))
    };
    let click = |w: &mut Workspace, label: &str| {
        draw(w, vec![]);
        let output = draw(w, vec![]);
        let p = gui_tests::shell_text_position(&output, label);
        for pressed in [true, false] {
            draw(
                w,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    };
    click(&mut workspace, "HTTP");
    click(&mut workspace, "FTP");
    assert_eq!(workspace.servers_category, servers::Section::Ftp);
    assert!(labels(&draw(&mut workspace, vec![])).contains("FTP file server"));
    click(&mut workspace, "FTP");
    click(&mut workspace, "HTTP");
    assert_eq!(workspace.http_root, "/owned/draft-folder");
    assert!(workspace.http.is_none() && workspace.http_start.is_none());
}

#[test]
fn tool_sections_keep_network_drafts_and_only_paint_the_selected_task() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    workspace.diagnostic_host = "owned.example.test".into();
    for &(category, _) in tools::Category::ALL {
        workspace.tools_category = category;
        let draw = |w: &mut Workspace| {
            gui_tests::shell_frame(&ctx, vec2(980., 640.), 1.5, vec![], |ui| w.tools_page(ui))
        };
        draw(&mut workspace);
        let output = draw(&mut workspace);
        let text = labels(&output);
        if category != tools::Category::Network {
            assert!(!text.contains("DNS lookup"));
        }
        assert_eq!(workspace.diagnostic_host, "owned.example.test");
    }
}

#[test]
fn all_service_forms_render_without_starting_a_listener() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    for category in servers::Section::ALL {
        workspace.servers_category = category;
        for size in [vec2(980., 640.), vec2(1360., 900.)] {
            for _ in 0..2 {
                let output =
                    gui_tests::shell_frame(&ctx, size, 1., vec![], |ui| workspace.servers_page(ui));
                assert!(!output.shapes.is_empty());
            }
        }
        assert!(workspace.http.is_none() && workspace.http_start.is_none());
    }
}

#[test]
fn sharing_folder_picker_does_not_overlap_the_next_form_row() {
    let (ctx, _root, mut workspace) = terminal_review_tests::owned_workspace();
    for width in [640., 700., 1000.] {
        let draw = |w: &mut Workspace| {
            gui_tests::shell_frame(&ctx, vec2(width, 620.), 1., vec![], |ui| w.servers_page(ui))
        };
        draw(&mut workspace);
        let output = draw(&mut workspace);
        let browse = gui_tests::shell_text_position(&output, "Browse");
        let listen = gui_tests::shell_text_position(&output, "Listen address");
        assert!(
            listen.y - browse.y >= 28.,
            "Picker overlaps next form row at {width}: {browse:?}, {listen:?}"
        );
    }
}

#[test]
fn remote_files_share_the_sidebar_instead_of_adding_a_second_panel() {
    let (ctx, _root, mut w) = terminal_review_tests::owned_workspace();
    w.start(Session {
        host: "127.0.0.1".into(),
        port: 1,
        ..Default::default()
    });
    let tab = &mut w.tabs[0];
    drop(tab.ssh.take());
    tab.ssh_connected = true;
    let browser = tab.browser.as_mut().unwrap();
    browser.loading = false;
    browser.entries = explorer_tests::remote_samples();
    let draw = |w: &mut Workspace| {
        gui_tests::shell_frame(&ctx, vec2(1360., 900.), 1., vec![], |ui| w.render(ui))
    };
    draw(&mut w);
    let output = draw(&mut w);
    let row = gui_tests::shell_text_position(&output, "photo.PNG");
    assert!(
        row.x < 340.,
        "Remote browser still occupies a second column: {row:?}"
    );
    assert!(
        !labels(&output).contains("Quick connect"),
        "Both sidebar contents are visible together"
    );
}

#[test]
fn connected_workspace_bar_stays_one_row_at_the_minimum_window() {
    let (ctx, _root, mut w) = terminal_review_tests::owned_workspace();
    w.start(Session {
        name: "A deliberately long active connection title for the compact tab strip".into(),
        host: "127.0.0.1".into(),
        port: 1,
        ..Default::default()
    });
    drop(w.tabs[0].ssh.take());
    for _ in 0..3 {
        gui_tests::shell_frame(&ctx, vec2(640., 480.), 1., vec![], |ui| w.render(ui));
    }
    assert!(
        egui::containers::panel::PanelState::load(&ctx, Id::new("menus")).is_some(),
        "App menus were combined with the connection tabs"
    );
    assert!(
        egui::containers::panel::PanelState::load(&ctx, Id::new("commands")).is_some(),
        "The separate app toolbar is missing"
    );
    for width in [260., 640., 1200.] {
        let mut height = 0.;
        for _ in 0..3 {
            gui_tests::shell_frame(&ctx, vec2(width, 480.), 1., vec![], |ui| {
                let start = ui.cursor().top();
                w.connection_commands(ui);
                height = ui.cursor().top() - start;
            });
        }
        assert!(
            height <= 40.,
            "Terminal controls wrapped at {width}: {height}"
        );
    }
}
