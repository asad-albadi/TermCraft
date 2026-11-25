use eframe::egui::{pos2, vec2};
use harbor_workspace::workspace::windows::{Geometry, ROOT, WorkspaceWindows};
#[test]
fn detach_reattach_keeps_one_transport() {
    let mut windows = WorkspaceWindows::default();
    let detached = windows.detach(42);
    assert_ne!(detached, ROOT);
    assert_eq!(windows.detach(42), detached);
    assert_eq!(windows.terminals(detached), vec![42]);
    windows.reattach(42, ROOT).unwrap();
    assert_eq!(windows.window(42), ROOT);
    assert!(windows.detached().is_empty());
    assert!(windows.reattach(42, detached).is_err());
}
#[test]
fn renamed_title_survives_server_title_change() {
    let mut windows = WorkspaceWindows::default();
    windows.rename(42, "My title".into());
    assert_eq!(windows.title(42, "remote shell", "original"), "My title");
    let window = windows.detach(42);
    windows.reattach(42, ROOT).unwrap();
    assert_eq!(windows.title(42, "new OSC", "original"), "My title");
    assert!(windows.terminals(window).is_empty());
    windows.rename(42, String::new());
    assert_eq!(windows.title(42, "new OSC", "original"), "new OSC");
}
#[test]
fn fullscreen_restore_keeps_bounds() {
    let mut windows = WorkspaceWindows::default();
    let geometry = Geometry {
        outer_position: pos2(21., 42.),
        inner_size: vec2(900., 600.),
    };
    windows.enter_fullscreen(ROOT, Some(geometry));
    windows.enter_fullscreen(ROOT, None); // must not overwrite restore bounds
    windows.leave_fullscreen(ROOT);
    assert!(windows.restore_after_ack(ROOT, true).is_none());
    assert_eq!(windows.restore_after_ack(ROOT, false), Some(geometry));
    assert!(windows.restore_after_ack(ROOT, false).is_none());
}
#[test]
fn moving_to_another_detached_window_compacts_the_old_window() {
    let mut windows = WorkspaceWindows::default();
    let first = windows.detach(1);
    let second = windows.detach(2);
    windows.reattach(1, second).unwrap();
    assert_eq!(windows.terminals(second), vec![1, 2]);
    assert!(!windows.detached().contains(&first));
    windows.remove(2);
    assert_eq!(windows.terminals(second), vec![1]);
    windows.remove(1);
    assert!(windows.detached().is_empty());
}
