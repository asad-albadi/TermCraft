use harbor_workspace::workspace::broadcast::{BroadcastGroup, InputOrigin};
#[test]
fn broadcast_sends_once_only_to_selected_targets() {
    let mut group = BroadcastGroup::default();
    group.select(1, true);
    group.select(3, true);
    group.select(1, true);
    assert_eq!(group.targets(1), vec![1, 3]);
    assert_eq!(
        group.targets(2),
        vec![2],
        "typing into an unselected source stays local"
    );
    let mut sent = vec![];
    let report = group.dispatch(b"cmd\r", |id, bytes| {
        assert_eq!(bytes, b"cmd\r");
        sent.push(id);
        if id == 3 {
            Err("Disconnected".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(sent, vec![1, 3]);
    assert_eq!(report.sent, vec![1]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, 3);
}
#[test]
fn auth_challenge_is_never_broadcast() {
    let mut group = BroadcastGroup::default();
    group.select(1, true);
    group.select(3, true);
    for origin in [
        InputOrigin::Authentication,
        InputOrigin::FileOperation,
        InputOrigin::ProtocolReply,
    ] {
        let report = group.dispatch_input(origin, b"sensitive", |_, _| {
            panic!("non-user input reached broadcast")
        });
        assert!(report.sent.is_empty());
    }
}
#[test]
fn removing_disconnected_targets_is_explicit_and_does_not_enroll_new_tabs() {
    let mut group = BroadcastGroup::default();
    group.select(1, true);
    group.select(3, true);
    assert_eq!(group.retain_connected(&[1, 2]), vec![3]);
    assert_eq!(group.selected(), vec![1]);
    assert_eq!(group.targets(1), vec![1]);
}
