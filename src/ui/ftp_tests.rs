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

#[tokio::test]
async fn graphical_ftp_session_connects_opens_edits_and_guards_tab_close() {
    let server = fixture::Endpoint::start(fixture::ServerOptions {
        tls: FtpTlsMode::Explicit,
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
