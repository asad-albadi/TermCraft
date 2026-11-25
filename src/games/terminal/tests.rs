use super::*;
#[test]
fn seven_declared_terminal_games() {
    assert_eq!(TerminalKind::ALL.len(), 7);
    assert!(
        !TerminalKind::ALL
            .iter()
            .any(|k| k.selector() == "typespeed")
    );
}
#[test]
fn deterministic_reset_and_invalid_input_atomicity() {
    for kind in TerminalKind::ALL {
        for seed in 0..16 {
            let mut s = TerminalSession::new(kind, seed);
            let initial = s.scene();
            assert_eq!(initial, TerminalSession::new(kind, seed).scene());
            let _ = s.apply(TerminalInput::Digit {
                cell: usize::MAX,
                value: 255,
            });
            assert_eq!(s.scene(), initial);
            assert!(!s.can_undo());
            s.reset();
            assert_eq!(s.scene(), initial)
        }
    }
}

#[test]
fn successful_inputs_undo_exactly_and_reset_matches_seed() {
    for kind in TerminalKind::ALL {
        let mut s = TerminalSession::new(kind, 8);
        let initial = s.scene();
        let input = match kind {
            TerminalKind::Soliterm => TerminalInput::Transfer {
                from: CardPile::Tableau(0),
                to: CardPile::Reserve(0),
                count: 1,
            },
            TerminalKind::Termwalk => TerminalInput::Rotate {
                cell: 0,
                clockwise: true,
            },
            TerminalKind::Ctris => TerminalInput::Tick,
            TerminalKind::Sudoku => {
                let TerminalScene::Sudoku(g) = s.scene() else {
                    unreachable!()
                };
                let c = g.cells.iter().position(|&v| v == 0).unwrap();
                let v = (1..=9)
                    .find(|&v| {
                        let mut candidate = s.clone();
                        candidate
                            .apply(TerminalInput::Digit { cell: c, value: v })
                            .is_ok()
                    })
                    .unwrap();
                TerminalInput::Digit { cell: c, value: v }
            }
            TerminalKind::Nimesweeper => TerminalInput::Flag { cell: 0 },
            TerminalKind::Ninvaders => TerminalInput::Start,
            TerminalKind::Greed => {
                let TerminalScene::Greed(g) = s.scene() else {
                    unreachable!()
                };
                let (dx, dy) = g.legal_directions[0];
                TerminalInput::Move { dx, dy }
            }
        };
        assert_eq!(s.apply(input), Ok(true));
        assert_ne!(s.scene(), initial);
        assert!(s.undo());
        assert_eq!(s.scene(), initial);
        assert!(!s.can_undo());
        s.apply(input).unwrap();
        s.reset();
        assert_eq!(s.scene(), initial);
    }
}
#[test]
fn history_is_bounded_noops_do_not_consume_and_rng_replays() {
    let mut s = TerminalSession::new(TerminalKind::Ctris, 0);
    s.apply(TerminalInput::Pause).unwrap();
    let n = s.history.len();
    for _ in 0..400 {
        s.apply(TerminalInput::Tick).unwrap();
    }
    assert_eq!(s.history.len(), n);
    s.apply(TerminalInput::Pause).unwrap();
    let initial = s.scene();
    s.apply(TerminalInput::Drop).unwrap();
    let future = s.scene();
    s.undo();
    assert_eq!(s.scene(), initial);
    s.apply(TerminalInput::Drop).unwrap();
    assert_eq!(s.scene(), future);
    let mut s = TerminalSession::new(TerminalKind::Termwalk, 0);
    for i in 0..300 {
        s.apply(TerminalInput::Rotate {
            cell: i % 25,
            clockwise: true,
        })
        .unwrap();
    }
    assert_eq!(s.history.len(), 128);
}
#[test]
fn terminal_boards_reject_moves_until_undo_or_reset() {
    let mut s = TerminalSession::new(TerminalKind::Ninvaders, 1);
    s.state = State::Invaders(invaders::Invaders {
        outcome: Outcome::Won,
        ..invaders::Invaders::new()
    });
    let before = s.scene();
    assert!(s.apply(TerminalInput::Start).is_err());
    assert_eq!(s.scene(), before);
    s.reset();
    assert_eq!(s.outcome(), Outcome::Playing);
}
#[test]
fn adversarial_inputs_are_bounded_and_never_panic() {
    let inputs = [
        TerminalInput::Digit {
            cell: usize::MAX,
            value: 255,
        },
        TerminalInput::Reveal { cell: usize::MAX },
        TerminalInput::Flag { cell: usize::MAX },
        TerminalInput::Chord { cell: usize::MAX },
        TerminalInput::Transfer {
            from: CardPile::Tableau(usize::MAX),
            to: CardPile::Reserve(usize::MAX),
            count: usize::MAX,
        },
        TerminalInput::Move {
            dx: i8::MIN,
            dy: i8::MAX,
        },
    ];
    for k in TerminalKind::ALL {
        let mut s = TerminalSession::new(k, u64::MAX);
        for i in inputs {
            let before = s.scene();
            assert!(s.apply(i).is_err());
            assert_eq!(s.scene(), before);
        }
    }
}

#[test]
fn paused_inputs_are_ignored_before_validation_without_history() {
    for kind in [
        TerminalKind::Ctris,
        TerminalKind::Nimesweeper,
        TerminalKind::Ninvaders,
    ] {
        let mut s = TerminalSession::new(kind, 0);
        if kind == TerminalKind::Ninvaders {
            s.apply(TerminalInput::Start).unwrap();
        }
        s.apply(TerminalInput::Pause).unwrap();
        let before = s.scene();
        let history = s.history.len();
        for input in [
            TerminalInput::Digit {
                cell: usize::MAX,
                value: 255,
            },
            TerminalInput::Move {
                dx: i8::MIN,
                dy: i8::MAX,
            },
            TerminalInput::Tick,
        ] {
            assert_eq!(s.apply(input), Ok(false));
            assert_eq!(s.scene(), before);
            assert_eq!(s.history.len(), history);
        }
        s.apply(TerminalInput::Pause).unwrap();
        assert!(
            s.apply(TerminalInput::Digit {
                cell: usize::MAX,
                value: 255
            })
            .is_err()
        );
    }
}
