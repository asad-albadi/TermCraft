use harbor_workspace::{
    model::{HighlightMatch, HighlightRule, Preferences, Session, Store},
    terminal::{Screen, highlight::HighlightRules},
};
fn literal(pattern: &str) -> HighlightRule {
    HighlightRule {
        pattern: pattern.into(),
        ..Default::default()
    }
}
#[test]
fn keyword_ranges_preserve_unicode_and_output() {
    let rules = HighlightRules::compile(&[literal("ERROR")]).unwrap();
    let mut screen = Screen::new(40, 3, 100);
    screen.feed("é ERROR ok".as_bytes());
    let before = screen.plain();
    let spans = rules.spans("é ERROR ok");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].range, 3..8);
    assert_eq!(screen.plain(), before);
    assert_eq!(screen.plain().trim_end(), "é ERROR ok");
}
#[test]
fn invalid_pattern_is_reported_before_save() {
    let mut preferences = Preferences::default();
    preferences
        .set_highlight_rules(vec![literal("ERROR")])
        .unwrap();
    let previous = preferences.highlight_rules.clone();
    let invalid = HighlightRule {
        pattern: "[".into(),
        kind: HighlightMatch::Regex,
        ..Default::default()
    };
    assert!(preferences.set_highlight_rules(vec![invalid]).is_err());
    assert_eq!(preferences.highlight_rules, previous);
}
#[test]
fn precedence_regex_case_and_resource_limits_are_deterministic() {
    let mut low = literal("ERROR");
    low.precedence = 1;
    let mut high = literal("err");
    high.case_sensitive = false;
    high.precedence = 9;
    high.dark.foreground = Some([1, 2, 3]);
    let rules = HighlightRules::compile(&[low, high]).unwrap();
    let spans = rules.spans("ERROR");
    assert_eq!(spans[0].range, 0..3);
    assert_eq!(spans[0].dark.foreground, Some([1, 2, 3]));
    assert_eq!(spans[1].range, 3..5);
    let regex = HighlightRule {
        pattern: r"é\s+ERROR".into(),
        kind: HighlightMatch::Regex,
        ..Default::default()
    };
    assert_eq!(
        HighlightRules::compile(&[regex])
            .unwrap()
            .spans("é ERROR ok")[0]
            .range,
        0..8
    );
    assert!(HighlightRules::compile(&vec![literal("x"); 17]).is_err());
    assert!(HighlightRules::compile(&[literal(&"x".repeat(257))]).is_err());
    assert!(HighlightRules::compile(&[literal("")]).is_err());
    assert!(
        HighlightRules::compile(&[HighlightRule {
            pattern: r"(a)\1".into(),
            kind: HighlightMatch::Regex,
            ..Default::default()
        }])
        .is_err()
    );
    let rules = HighlightRules::compile(&[literal("x")]).unwrap();
    assert!(rules.spans(&"x".repeat(100000)).len() <= 128);
}
#[test]
fn old_settings_and_session_override_roundtrip() {
    let old: Store = serde_json::from_str(
        r#"{"sessions":[{"name":"old","protocol":"Local"}],"preferences":{"font_size":14.0}}"#,
    )
    .unwrap();
    assert!(old.preferences.highlight_rules.is_empty());
    assert!(old.preferences.highlight_enabled);
    assert_eq!(old.sessions[0].highlight_enabled, None);
    let session = Session {
        highlight_enabled: Some(false),
        ..Default::default()
    };
    let restored: Session =
        serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(restored.highlight_enabled, Some(false));
}
