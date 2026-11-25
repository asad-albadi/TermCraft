use harbor_workspace::macros::{Action, Context, Event, Player, Recorder, Script, Target};
use std::time::Duration;

fn target() -> Target {
    Target {
        tab: 7,
        generation: 2,
    }
}
fn context<'a>(now: u64, output: &'a str) -> Context<'a> {
    Context {
        target: target(),
        now: Duration::from_millis(now),
        authenticated: true,
        focused: true,
        output,
    }
}
fn script(events: Vec<Event>) -> Script {
    Script { version: 1, events }
}

#[test]
fn macro_playback_obeys_recorded_timing_and_cancellation() {
    let mut recorder = Recorder::new(target(), Duration::ZERO);
    assert!(
        recorder
            .record(context(0, ""), Action::Text("one".into()))
            .unwrap()
    );
    assert!(
        recorder
            .record(
                context(100, ""),
                Action::Key {
                    code: 30,
                    pressed: true
                }
            )
            .unwrap()
    );
    assert!(
        recorder
            .record(
                context(250, ""),
                Action::Key {
                    code: 30,
                    pressed: false
                }
            )
            .unwrap()
    );
    let mut player = Player::new(target(), recorder.finish(), &[], Duration::ZERO).unwrap();
    assert_eq!(
        player.tick(context(0, "")).unwrap(),
        vec![Action::Text("one".into())]
    );
    assert!(player.tick(context(99, "")).unwrap().is_empty());
    assert_eq!(
        player.tick(context(100, "")).unwrap(),
        vec![Action::Key {
            code: 30,
            pressed: true
        }]
    );
    assert!(player.tick(context(249, "")).unwrap().is_empty());
    assert_eq!(
        player.tick(context(250, "")).unwrap(),
        vec![Action::Key {
            code: 30,
            pressed: false
        }]
    );
    assert!(player.finished());
    let mut player = Player::new(
        target(),
        script(vec![
            Event::Wait { millis: 100 },
            Event::Input(Action::Text("never".into())),
        ]),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(player.cancel(), vec![Action::ReleaseAll]);
    assert!(player.tick(context(500, "")).unwrap().is_empty());
}

#[test]
fn macro_never_records_auth_challenges_or_replays_to_replaced_session() {
    let mut recorder = Recorder::new(target(), Duration::ZERO);
    let mut auth = context(0, "");
    auth.authenticated = false;
    assert!(
        !recorder
            .record(auth, Action::Text("secret".into()))
            .unwrap()
    );
    assert!(recorder.finish().events.is_empty());
    let mut player = Player::new(
        target(),
        script(vec![
            Event::Wait { millis: 100 },
            Event::Input(Action::Text("command".into())),
        ]),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    let mut replaced = context(100, "");
    replaced.target.generation += 1;
    assert!(player.tick(replaced).is_err());
    assert!(player.finished());
    assert_eq!(player.take_cleanup(), vec![Action::ReleaseAll]);
    assert!(player.tick(context(100, "")).unwrap().is_empty());
    let mut player = Player::new(
        target(),
        script(vec![Event::Input(Action::Text("command".into()))]),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    assert!(player.tick(auth).is_err());
    assert_eq!(player.take_cleanup(), vec![Action::ReleaseAll]);
}

#[test]
fn pattern_wait_observes_stream_fragments_and_timeout() {
    let script = script(vec![
        Event::WaitPattern {
            pattern: "ready>".into(),
            timeout_millis: 1000,
        },
        Event::Input(Action::Text("go".into())),
    ]);
    let mut player = Player::new(target(), script.clone(), &[], Duration::ZERO).unwrap();
    assert!(player.tick(context(0, "rea")).unwrap().is_empty());
    assert_eq!(
        player.tick(context(50, "dy>")).unwrap(),
        vec![Action::Text("go".into())]
    );
    let mut player = Player::new(target(), script, &[], Duration::ZERO).unwrap();
    assert!(player.tick(context(1000, "")).is_err());
    assert_eq!(player.take_cleanup(), vec![Action::ReleaseAll]);
}

#[test]
fn focus_wait_and_nested_calls_use_one_target_and_reject_cycles() {
    let child = script(vec![
        Event::Input(Action::Pointer {
            x: 32768,
            y: 65535,
            buttons: 1,
        }),
        Event::Wait { millis: 25 },
    ]);
    let top = script(vec![
        Event::WaitFocus {
            timeout_millis: 100,
        },
        Event::Call {
            name: "child".into(),
        },
        Event::Input(Action::ReleaseAll),
    ]);
    let mut player =
        Player::new(target(), top, &[("child".into(), child)], Duration::ZERO).unwrap();
    let mut unfocused = context(0, "");
    unfocused.focused = false;
    assert!(player.tick(unfocused).unwrap().is_empty());
    assert_eq!(
        player.tick(context(50, "")).unwrap(),
        vec![Action::Pointer {
            x: 32768,
            y: 65535,
            buttons: 1
        }]
    );
    assert!(player.tick(context(74, "")).unwrap().is_empty());
    assert_eq!(
        player.tick(context(75, "")).unwrap(),
        vec![Action::ReleaseAll]
    );
    let recursive = script(vec![Event::Call {
        name: "again".into(),
    }]);
    assert!(
        Player::new(
            target(),
            recursive.clone(),
            &[("again".into(), recursive)],
            Duration::ZERO
        )
        .is_err()
    );
}

#[test]
fn playback_limits_catchup_and_rejects_unbounded_or_unknown_scripts() {
    let mut player = Player::new(
        target(),
        script(
            (0..200)
                .map(|_| Event::Input(Action::Text("x".into())))
                .collect(),
        ),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(player.tick(context(10_000, "")).unwrap().len(), 64);
    assert!(!player.finished());
    assert_eq!(player.tick(context(10_000, "")).unwrap().len(), 64);
    assert!(
        Player::new(
            target(),
            Script {
                version: 99,
                events: vec![]
            },
            &[],
            Duration::ZERO
        )
        .is_err()
    );
    assert!(
        Player::new(
            target(),
            script(vec![Event::Input(Action::Text(
                "x".repeat(1024 * 1024 + 1)
            ))]),
            &[],
            Duration::ZERO
        )
        .is_err()
    );
    assert!(
        Player::new(
            target(),
            script(vec![Event::Call {
                name: "missing".into()
            }]),
            &[],
            Duration::ZERO
        )
        .is_err()
    );
}

#[test]
fn completion_releases_held_keys_and_pointer_buttons() {
    let mut player = Player::new(
        target(),
        script(vec![
            Event::Input(Action::Key {
                code: 42,
                pressed: true,
            }),
            Event::Input(Action::Pointer {
                x: 1,
                y: 2,
                buttons: 1,
            }),
        ]),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    let actions = player.tick(context(0, "")).unwrap();
    assert_eq!(actions.last(), Some(&Action::ReleaseAll));
    assert!(player.finished());
    assert!(player.tick(context(1, "")).unwrap().is_empty());
}

#[test]
fn pattern_wait_does_not_reuse_output_from_before_command_delivery() {
    let mut player = Player::new(
        target(),
        script(vec![
            Event::Input(Action::Text("command".into())),
            Event::WaitPattern {
                pattern: "done".into(),
                timeout_millis: 100,
            },
            Event::Input(Action::Text("next".into())),
        ]),
        &[],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(
        player.tick(context(0, "done")).unwrap(),
        vec![Action::Text("command".into())]
    );
    assert!(player.tick(context(50, "")).unwrap().is_empty());
    assert_eq!(
        player.tick(context(60, "done")).unwrap(),
        vec![Action::Text("next".into())]
    );
}

#[test]
fn empty_call_expansion_has_a_work_budget() {
    let empty = script(vec![]);
    let fanout = script(
        (0..4096)
            .map(|_| Event::Call {
                name: "empty".into(),
            })
            .collect(),
    );
    let root = script(vec![Event::Call {
        name: "fanout".into(),
    }]);
    assert!(
        Player::new(
            target(),
            root,
            &[("empty".into(), empty), ("fanout".into(), fanout)],
            Duration::ZERO
        )
        .is_err()
    );
}

#[test]
fn consecutive_pattern_waits_consume_coalesced_and_split_utf8_output_in_order() {
    for complete in [true, false] {
        let mut player = Player::new(
            target(),
            script(vec![
                Event::WaitPattern {
                    pattern: "ready".into(),
                    timeout_millis: 100,
                },
                Event::WaitPattern {
                    pattern: "πrompt".into(),
                    timeout_millis: 100,
                },
                Event::Input(Action::Text("done".into())),
            ]),
            &[],
            Duration::ZERO,
        )
        .unwrap();
        let actions = if complete {
            player.tick(context(0, "ready πrompt")).unwrap()
        } else {
            assert!(player.tick(context(0, "ready πr")).unwrap().is_empty());
            player.tick(context(25, "ompt")).unwrap()
        };
        assert_eq!(actions, vec![Action::Text("done".into())]);
    }
}
