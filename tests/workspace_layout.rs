use eframe::egui::{Rect, pos2};
use harbor_workspace::workspace::layout::{
    LayoutPreferences, LayoutPreset, LayoutState, LayoutTree, MIN_HEIGHT, MIN_WIDTH, Orientation,
};
#[test]
fn four_panes_cover_bounds_without_overlap() {
    let prefs = LayoutPreferences {
        preset: LayoutPreset::Four,
        ..Default::default()
    };
    let tree = LayoutTree::preset(&[11, 22, 33, 44], &prefs).unwrap();
    let bounds = Rect::from_min_max(pos2(10., 20.), pos2(1210., 820.));
    let panes = tree.layout(bounds);
    assert_eq!(panes.len(), 4);
    for (i, pane) in panes.iter().enumerate() {
        assert!(bounds.contains_rect(pane.rect));
        assert!(pane.rect.width() >= MIN_WIDTH && pane.rect.height() >= MIN_HEIGHT);
        for other in panes.iter().skip(i + 1) {
            assert!(!pane.rect.intersect(other.rect).is_positive());
        }
    }
    let area: f32 = panes.iter().map(|p| p.rect.area()).sum::<f32>()
        + tree
            .dividers(bounds)
            .iter()
            .map(|d| d.rect.area())
            .sum::<f32>();
    assert!((area - bounds.area()).abs() < 1.);
}
#[test]
fn closing_pane_preserves_other_terminal_ids() {
    let root = LayoutTree::preset(
        &[11, 22, 33, 44],
        &LayoutPreferences {
            preset: LayoutPreset::Four,
            ..Default::default()
        },
    );
    let mut state = LayoutState {
        root,
        active: Some(33),
    };
    state.remove(22);
    assert_eq!(state.root.as_ref().unwrap().ids(), vec![11, 33, 44]);
    assert_eq!(state.active, Some(33));
    let restored: LayoutState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored.active, Some(33));
    assert_eq!(restored.root.unwrap().ids(), vec![11, 33, 44]);
    state.remove(33);
    assert!(state.active.is_some_and(|id| id != 33));
    state.remove(11);
    state.remove(44);
    assert!(state.root.is_none());
    assert!(state.active.is_none());
}
#[test]
fn orientation_divider_resize_and_minimum_bounds_are_consistent() {
    let bounds = Rect::from_min_max(pos2(0., 0.), pos2(1000., 700.));
    let mut tree = LayoutTree::preset(
        &[1, 2],
        &LayoutPreferences {
            preset: LayoutPreset::Rows,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(tree.dividers(bounds)[0].axis, Orientation::Vertical);
    assert_eq!(tree.layout(bounds)[0].rect.width(), 1000.);
    tree.set_ratio(&[], 0.8).unwrap();
    let panes = tree.layout(bounds);
    assert!(panes[0].rect.height() > panes[1].rect.height());
    assert!(panes[1].rect.height() >= MIN_HEIGHT);
    assert!(!tree.fits(Rect::from_min_max(pos2(0., 0.), pos2(100., 100.))));
    assert!(tree.set_ratio(&[], f32::NAN).is_err());
    let preferences = LayoutPreferences {
        preset: LayoutPreset::Four,
        ratios: [0.6, 0.3, 0.7],
    };
    let restored: LayoutPreferences =
        serde_json::from_str(&serde_json::to_string(&preferences).unwrap()).unwrap();
    assert_eq!(restored, preferences);
}
#[test]
fn presets_only_use_distinct_available_terminal_ids() {
    let prefs = LayoutPreferences {
        preset: LayoutPreset::Four,
        ..Default::default()
    };
    assert!(LayoutTree::preset(&[], &prefs).is_none());
    assert_eq!(
        LayoutTree::preset(&[1, 1, 2, 3, 4, 5], &prefs)
            .unwrap()
            .ids(),
        vec![1, 2, 3, 4]
    );
    let old: harbor_workspace::model::Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.layout.preset, LayoutPreset::Single);
}
