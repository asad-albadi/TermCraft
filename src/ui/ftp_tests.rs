#[allow(dead_code)]
#[path = "../../tests/support/ftp_server.rs"]
mod fixture;

use super::*;
use crate::ui::Workspace;
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, epaint::Shape};
use harbor_workspace::{model::Session, protocols::ftp::FtpTlsMode};
use std::time::Instant;

fn render(
    workspace: &mut Workspace,
    context: &Context,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    render_input(
        workspace,
        context,
        RawInput {
            events,
            ..Default::default()
        },
    )
}

fn render_input(
    workspace: &mut Workspace,
    context: &Context,
    mut input: RawInput,
) -> Vec<egui::epaint::ClippedShape> {
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280., 960.)));
    let mut output = context.run_ui(input, |ui| {
        eframe::App::ui(workspace, ui, &mut eframe::Frame::_new_kittest())
    });
    output.textures_delta.clear();
    output.shapes
}

fn find(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<Pos2> {
    fn position(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| position(shape, label)),
            _ => None,
        }
    }
    shapes
        .iter()
        .filter_map(|shape| position(&shape.shape, label))
        .max_by(|left, right| left.y.total_cmp(&right.y))
}

fn click(workspace: &mut Workspace, ctx: &Context, position: Pos2) {
    for pressed in [true, false] {
        render(
            workspace,
            ctx,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

async fn wait_for_text(workspace: &mut Workspace, ctx: &Context, label: &str) -> Pos2 {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let shapes = render(workspace, ctx, vec![]);
        if let Some(position) = find(&shapes, label) {
            return position;
        }
        assert!(
            Instant::now() < deadline,
            "FTP workspace omitted {label:?}. Status: {}",
            workspace.tabs[0].ftp.as_ref().unwrap().status(),
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

async fn open_owned_ftp_document(
    tls: FtpTlsMode,
) -> (fixture::Endpoint, tempfile::TempDir, Context, Workspace) {
    let server = fixture::Endpoint::start(fixture::ServerOptions {
        tls,
        ..Default::default()
    })
    .await;
    server.write_file("/fixture-note.txt", b"Fixture original text");
    let options = server.options();
    let data = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut workspace = Workspace::new_context(&ctx, None, data.path().to_owned());
    workspace.store.preferences.confirm_session_close = false;
    workspace.start(Session {
        protocol: "FTP".into(),
        name: "Fixture FTPS".into(),
        host: options.host,
        port: options.port,
        user: options.username,
        ftp_options: SessionOptions {
            tls,
            protect_data: options.protect_data,
            certificate_authority: options.certificate_authority,
            ..SessionOptions::embedded()
        },
        ..Default::default()
    });
    assert_eq!(workspace.tabs.len(), 1);
    workspace.tabs[0]
        .ftp
        .as_mut()
        .unwrap()
        .password
        .push_str(options.password.unwrap().expose());
    let connect = wait_for_text(&mut workspace, &ctx, "Connect").await;
    click(&mut workspace, &ctx, connect);
    let entry = wait_for_text(&mut workspace, &ctx, "fixture-note.txt").await;
    click(&mut workspace, &ctx, entry);
    let edit = wait_for_text(&mut workspace, &ctx, "Edit…").await;
    click(&mut workspace, &ctx, edit);
    let document = wait_for_text(&mut workspace, &ctx, "Fixture original text").await;
    click(&mut workspace, &ctx, document);
    (server, data, ctx, workspace)
}

#[tokio::test]
async fn ftp_listing_draws_distinct_icons_for_directory_file_and_link() {
    use crate::ui::explorer_tests::{REMOTE_LABELS, assert_distinct_icons, remote_samples};
    let (_server, _root, ctx, mut w) = open_owned_ftp_document(FtpTlsMode::Plain).await;
    ctx.enable_accesskit();
    let ftp = w.tabs[0].ftp.as_mut().unwrap();
    ftp.actor.as_mut().unwrap().browser.entries = remote_samples();
    let draw = |ftp: &mut State| {
        crate::ui::gui_tests::shell_frame(&ctx, egui::vec2(640., 900.), 1., vec![], |ui| {
            ftp.browser(ui)
        })
    };
    draw(ftp);
    assert_distinct_icons(&draw(ftp), REMOTE_LABELS);
}

#[tokio::test]
async fn ftp_small_wheel_steps_preserve_virtual_row_geometry_and_file_selection() {
    let (_server, _data, ctx, mut workspace) = open_owned_ftp_document(FtpTlsMode::Plain).await;
    let ftp = workspace.tabs[0].ftp.as_mut().unwrap();
    ftp.actor.as_mut().unwrap().browser.entries = (0..120)
        .map(|i| {
            let name = format!("item-{i:03}.txt");
            RemoteEntry {
                path: RemotePath::from_bytes(format!("/{name}").into_bytes()).unwrap(),
                display_name: name,
                kind: EntryKind::File,
                size: Some(5),
            }
        })
        .collect();
    let draw = |ftp: &mut State, events| {
        let mut before = 0.;
        let output =
            crate::ui::gui_tests::shell_frame(&ctx, egui::vec2(640., 550.), 1., events, |ui| {
                let id = ui.make_persistent_id(egui::IdSalt::new("ftp-list"));
                before = egui::scroll_area::State::load(&ctx, id)
                    .unwrap_or_default()
                    .offset
                    .y;
                ftp.browser(ui);
            });
        (output, before)
    };
    let (first, _) = draw(ftp, vec![]);
    let pos = crate::ui::gui_tests::shell_text_position(&first, "item-000.txt");
    let next = crate::ui::gui_tests::shell_text_position(&first, "item-001.txt");
    let stride = next.y - pos.y;
    let mut clicked = false;
    for _ in 0..80 {
        let (frame, offset) = draw(
            ftp,
            vec![
                Event::PointerMoved(next),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -7.),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        for shape in &frame.shapes {
            let Shape::Text(t) = &shape.shape else {
                continue;
            };
            let Some(index) = t
                .galley
                .text()
                .strip_prefix("item-")
                .and_then(|s| s.strip_suffix(".txt"))
                .and_then(|s| s.parse::<usize>().ok())
            else {
                continue;
            };
            let center = t.pos + t.galley.size() * 0.5;
            if !shape.clip_rect.contains(center) {
                continue;
            }
            assert!(
                (center.y - (pos.y + index as f32 * stride - offset)).abs() < 1.6,
                "row {index}, offset {offset}, center {center:?}, stride {stride}"
            );
            if index == 10 && offset > 4. * stride && center.y > shape.clip_rect.top() + stride {
                for down in [true, false] {
                    draw(
                        ftp,
                        vec![
                            Event::PointerMoved(center),
                            crate::ui::gui_tests::shell_pointer(center, down),
                        ],
                    );
                }
                assert_eq!(
                    ftp.selected.as_ref().unwrap().path.as_bytes(),
                    b"/item-010.txt"
                );
                clicked = true;
                break;
            }
        }
        if clicked {
            break;
        }
    }
    assert!(
        clicked,
        "small wheel steps did not expose the intended filename"
    );
}

#[tokio::test]
async fn graphical_ftp_session_connects_opens_edits_and_guards_tab_close() {
    let (server, _data, ctx, mut workspace) = open_owned_ftp_document(FtpTlsMode::Explicit).await;
    workspace.store.preferences.confirm_session_close = true;
    workspace.close_tab(0);
    assert!(workspace.pending_session_close.is_some());
    render(&mut workspace, &ctx, vec![Event::Text(" changed".into())]);
    assert!(
        workspace.tabs[0]
            .ftp
            .as_ref()
            .unwrap()
            .has_pending_documents()
    );

    let mut approval = ctx.run_ui(RawInput::default(), |ui| {
        workspace.resolve_session_close(true, ui.ctx());
    });
    approval.textures_delta.clear();
    assert_eq!(workspace.tabs.len(), 1);
    assert!(workspace.pending_remote_tab_close.is_some());
    wait_for_text(
        &mut workspace,
        &ctx,
        "Save or discard changes before closing this FTP document.",
    )
    .await;
    let shapes = render(&mut workspace, &ctx, vec![]);
    assert!(find(&shapes, "Remote documents have unsaved or pending work").is_none());
    let keep = wait_for_text(&mut workspace, &ctx, "Keep editing").await;
    click(&mut workspace, &ctx, keep);
    assert!(workspace.pending_remote_tab_close.is_none());
    assert_eq!(workspace.tabs.len(), 1);

    workspace.store.preferences.confirm_session_close = false;
    let mut input = RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    render_input(&mut workspace, &ctx, input);
    assert!(workspace.pending_remote_exit);
    assert!(!workspace.allow_remote_exit);
    let discard = wait_for_text(&mut workspace, &ctx, "Discard and close").await;
    click(&mut workspace, &ctx, discard);
    assert!(!workspace.pending_remote_exit);
    assert!(workspace.allow_remote_exit);
    assert_eq!(
        server.read_file("/fixture-note.txt").unwrap(),
        b"Fixture original text"
    );
    assert!(server.evidence().encrypted_data_connections >= 1);
}

#[test]
fn ftp_recursive_delete_requires_explicit_contents_choice_and_exact_path() {
    let path = RemotePath::from_bytes("/日本語/folder".as_bytes().to_vec()).unwrap();
    let pending = |contents, typed| Pending::Delete {
        path: path.clone(),
        directory: true,
        contents,
        typed,
    };
    assert!(matches!(
        pending(false, String::new()).operation(ConflictPolicy::Ask),
        Ok(TransferOperation::Remove { .. })
    ));
    assert!(
        pending(true, path.display())
            .operation(ConflictPolicy::Ask)
            .is_err()
    );
    assert!(
        matches!(pending(true, path.identity()).operation(ConflictPolicy::Ask), Ok(TransferOperation::RemoveTree { remote, confirmed }) if remote == path && confirmed == path)
    );
}

#[test]
fn ftp_confirmation_carries_the_source_kind_into_transfer_jobs() {
    let root = RemotePath::from_bytes(b"/remote".to_vec()).unwrap();
    let local = PathBuf::from("selected");
    let upload = |folder| Pending::Upload {
        local: local.clone(),
        directory: root.clone(),
        name: "target".to_owned(),
        folder,
    };
    let download = |folder| Pending::Download {
        remote: root.clone(),
        local: local.clone(),
        folder,
    };
    assert!(matches!(
        upload(false).operation(ConflictPolicy::Ask),
        Ok(TransferOperation::Upload { .. })
    ));
    assert!(matches!(
        upload(true).operation(ConflictPolicy::Ask),
        Ok(TransferOperation::UploadFolder { .. })
    ));
    assert!(matches!(
        download(false).operation(ConflictPolicy::Ask),
        Ok(TransferOperation::Download { .. })
    ));
    assert!(matches!(
        download(true).operation(ConflictPolicy::Ask),
        Ok(TransferOperation::DownloadFolder { .. })
    ));
}

#[tokio::test]
async fn unverified_ftp_recovery_can_be_dismissed_and_reopened_without_losing_edits() {
    let (server, _data, ctx, mut workspace) = open_owned_ftp_document(FtpTlsMode::Plain).await;
    render(&mut workspace, &ctx, vec![Event::Text(" retained".into())]);
    let ftp = workspace.tabs[0].ftp.as_mut().unwrap();
    let original = ftp.editor.as_ref().unwrap().document.snapshot().text;
    assert!(original.contains("retained"));
    ftp.disconnect();
    wait_for_text(&mut workspace, &ctx, "Connect").await;
    let ftp = workspace.tabs[0].ftp.as_mut().unwrap();
    ftp.password
        .push_str(server.options().password.unwrap().expose());
    ftp.connect();
    let cancel = wait_for_text(&mut workspace, &ctx, "Cancel comparison").await;
    click(&mut workspace, &ctx, cancel);
    let shapes = render(&mut workspace, &ctx, vec![]);
    assert!(find(&shapes, "Cancel comparison").is_none());
    assert_eq!(
        workspace.tabs[0]
            .ftp
            .as_ref()
            .unwrap()
            .editor
            .as_ref()
            .unwrap()
            .document
            .snapshot()
            .text,
        original
    );
    let reopen = wait_for_text(&mut workspace, &ctx, "Compare retained FTP edits…").await;
    click(&mut workspace, &ctx, reopen);
    wait_for_text(&mut workspace, &ctx, "Cancel comparison").await;
    assert_eq!(
        server.read_file("/fixture-note.txt").unwrap(),
        b"Fixture original text"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn mixed_ssh_and_ftp_close_keeps_both_editors_available() {
    use harbor_workspace::remote_files::editor::DocumentStatus;
    let (ftp, data, ctx, mut workspace) = open_owned_ftp_document(FtpTlsMode::Explicit).await;
    render(&mut workspace, &ctx, vec![Event::Text(" unsaved".into())]);
    let ssh = crate::ui::gui_tests::remote_fixture::SshServer::start(true);
    let config = ssh.config(99, true);
    std::fs::create_dir_all(data.path().join("ssh")).unwrap();
    std::fs::copy(&config.app_known_hosts, data.path().join("ssh/known_hosts")).unwrap();
    let path = ssh.root.join("original.txt");
    std::fs::write(&path, "original").unwrap();
    workspace.start(config.session);
    let tab = workspace.active;
    let deadline = Instant::now() + Duration::from_secs(10);
    while workspace.tabs[tab].browser.as_ref().unwrap().loading {
        workspace.tabs[tab].poll();
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    workspace.open_remote_editor(
        workspace.tabs[tab].id,
        RemotePath::from_bytes(path.to_str().unwrap().as_bytes().to_vec()).unwrap(),
    );
    while workspace.remote_editors[0].document.details().0 != DocumentStatus::Ready {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    workspace.remote_editors[0]
        .document
        .set_text("edited")
        .unwrap();
    workspace.pending_remote_exit = true;
    workspace.begin_ftp_document_close(None);
    let shapes = render(&mut workspace, &ctx, vec![]);
    assert!(find(&shapes, "Keep editing").is_some());
    wait_for_text(&mut workspace, &ctx, "Cancel close").await;
    assert!(ctx.memory(|m| m.top_modal_layer()).is_none());
    // Raise the FTP document before activating its own cancellation control.
    let ftp_window = egui::LayerId::new(
        egui::Order::Middle,
        Id::new(("ftp-editor", workspace.tabs[0].id)),
    );
    ctx.move_to_top(ftp_window);
    let keep = wait_for_text(&mut workspace, &ctx, "Keep editing").await;
    click(&mut workspace, &ctx, keep);
    assert!(!workspace.pending_remote_exit);
    assert!(
        workspace.tabs[0]
            .ftp
            .as_ref()
            .unwrap()
            .has_pending_documents()
    );
    assert!(workspace.remote_editors[0].document.details().1);
    assert_eq!(std::fs::read(path).unwrap(), b"original");
    assert_eq!(
        ftp.read_file("/fixture-note.txt").unwrap(),
        b"Fixture original text"
    );
}
