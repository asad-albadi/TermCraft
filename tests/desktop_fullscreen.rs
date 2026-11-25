use eframe::egui::{pos2, vec2};
use harbor_workspace::workspace::windows::{Geometry, ROOT, WorkspaceWindows};

#[test]
fn session_fullscreen_waits_for_connection_and_preserves_restore_geometry() {
    let mut windows = WorkspaceWindows::default();
    let geometry = Geometry {
        outer_position: pos2(24., 42.),
        inner_size: vec2(960., 640.),
    };
    let mut requested = true;
    assert!(!windows.session_fullscreen(
        ROOT,
        eframe::egui::ViewportId::ROOT,
        &mut requested,
        false,
        Some(geometry)
    ));
    assert!(requested);
    assert!(!windows.fullscreen(ROOT));
    assert!(windows.session_fullscreen(
        ROOT,
        eframe::egui::ViewportId::ROOT,
        &mut requested,
        true,
        Some(geometry)
    ));
    assert!(!requested);
    windows.leave_fullscreen(ROOT);
    assert_eq!(windows.restore_after_ack(ROOT, false), Some(geometry));
    // A user leaving fullscreen must not be put back on every repaint.
    assert!(!windows.session_fullscreen(
        ROOT,
        eframe::egui::ViewportId::ROOT,
        &mut requested,
        true,
        Some(geometry)
    ));
    assert!(!windows.fullscreen(ROOT));
}

#[test]
fn session_fullscreen_targets_detached_window_without_toggling_existing_fullscreen() {
    let mut windows = WorkspaceWindows::default();
    let detached = windows.detach(42);
    let mut requested = true;
    assert!(windows.session_fullscreen(
        detached,
        harbor_workspace::workspace::windows::viewport_id(detached),
        &mut requested,
        true,
        None
    ));
    assert!(windows.fullscreen(detached));
    assert!(!windows.fullscreen(ROOT));
    let mut second = true;
    assert!(!windows.session_fullscreen(
        detached,
        harbor_workspace::workspace::windows::viewport_id(detached),
        &mut second,
        true,
        None
    ));
    assert!(!second);
    assert!(windows.fullscreen(detached));
    let mut disabled = false;
    assert!(!windows.session_fullscreen(
        ROOT,
        eframe::egui::ViewportId::ROOT,
        &mut disabled,
        true,
        None
    ));
}

#[test]
fn embedded_detached_view_does_not_fullscreen_the_root_or_consume_its_request() {
    use eframe::egui::{
        Context, RawInput, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId,
    };
    let ctx = Context::default();
    ctx.set_embed_viewports(true);
    let mut windows = WorkspaceWindows::default();
    let detached = windows.detach(42);
    let mut requested = true;
    let mut observed = false;
    let mut output = ctx.run_ui(RawInput::default(), |ui| {
        ui.ctx().show_viewport_immediate(
            ViewportId::from_hash_of(("termcraft-window", detached)),
            ViewportBuilder::default(),
            |ui, class| {
                observed = true;
                assert!(matches!(class, ViewportClass::EmbeddedWindow));
                assert_eq!(ui.ctx().viewport_id(), ViewportId::ROOT);
                if windows.session_fullscreen(
                    detached,
                    ui.ctx().viewport_id(),
                    &mut requested,
                    true,
                    None,
                ) {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Fullscreen(true));
                }
            },
        );
    });
    output.textures_delta.clear();
    assert!(observed);
    assert!(
        requested,
        "Request must survive until a native detached viewport exists"
    );
    assert!(!windows.fullscreen(ROOT));
    assert!(!windows.fullscreen(detached));
    assert!(!output.viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, ViewportCommand::Fullscreen(true)))
    }));
}
