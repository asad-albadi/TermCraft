use super::*;
#[test]
fn all_kinds_seed_replay_and_reset() {
    for kind in PuzzleKind::ALL {
        for seed in [0, 1, 42, u64::MAX] {
            let mut game = PuzzleSession::new(kind, seed);
            assert_eq!(game.scene(), PuzzleSession::new(kind, seed).scene());
            assert_eq!(game.outcome(), Outcome::Playing);
            let start = game.scene();
            let action = match kind {
                PuzzleKind::Net => PuzzleInput::Rotate {
                    cell: 0,
                    clockwise: true,
                },
                PuzzleKind::Mines => PuzzleInput::Flag { cell: 0 },
                PuzzleKind::Solo => {
                    let PuzzleScene::Solo(s) = game.scene() else {
                        panic!()
                    };
                    PuzzleInput::Digit {
                        cell: s.givens.iter().position(|x| !x).unwrap(),
                        value: 1,
                    }
                }
                PuzzleKind::Guess => PuzzleInput::Peg { slot: 0, colour: 1 },
                PuzzleKind::Map => {
                    let PuzzleScene::Map(s) = game.scene() else {
                        panic!()
                    };
                    PuzzleInput::Colour {
                        region: s.givens.iter().position(|x| !x).unwrap(),
                        colour: 1,
                    }
                }
                PuzzleKind::Untangle => PuzzleInput::Move {
                    vertex: 0,
                    x: 500,
                    y: 500,
                },
            };
            assert!(game.apply(action).unwrap());
            assert_ne!(game.scene(), start);
            assert!(game.undo());
            assert_eq!(game.scene(), start);
            game.apply(action).unwrap();
            game.reset();
            assert_eq!(game.scene(), start);
        }
    }
}
#[test]
fn rejects_wrong_inputs_without_history() {
    let mut g = PuzzleSession::new(PuzzleKind::Guess, 4);
    assert!(g.apply(PuzzleInput::Reveal { cell: 0 }).is_err());
    assert!(
        g.apply(PuzzleInput::Peg {
            slot: usize::MAX,
            colour: 1
        })
        .is_err()
    );
    assert!(g.apply(PuzzleInput::Peg { slot: 0, colour: 7 }).is_err());
    assert!(g.apply(PuzzleInput::Submit).is_err());
    assert!(!g.undo());
}
#[test]
fn bounded_undo_and_finished_board_recovery() {
    let mut g = PuzzleSession::new(PuzzleKind::Net, 4);
    for _ in 0..400 {
        g.apply(PuzzleInput::Lock { cell: 0 }).unwrap();
    }
    let mut count = 0;
    while g.undo() {
        count += 1;
    }
    assert_eq!(count, PuzzleSession::UNDO_LIMIT);
    let mut g = PuzzleSession::new(PuzzleKind::Guess, 23);
    let State::Guess(ref mut s) = g.state else {
        panic!()
    };
    // Test-only known code; production scene never leaks this while playing.
    let answer = s.solution_for_test();
    for (slot, colour) in answer.into_iter().enumerate() {
        g.apply(PuzzleInput::Peg { slot, colour }).unwrap();
    }
    g.apply(PuzzleInput::Submit).unwrap();
    assert_eq!(g.outcome(), Outcome::Won);
    assert!(g.apply(PuzzleInput::Peg { slot: 0, colour: 1 }).is_err());
    assert!(g.undo());
    assert_eq!(g.outcome(), Outcome::Playing);
    let PuzzleScene::Guess(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.solution, None);
}
#[test]
fn mine_fatal_move_undo_retains_death_count() {
    let mut g = PuzzleSession::new(PuzzleKind::Mines, 5);
    g.apply(PuzzleInput::Reveal { cell: 40 }).unwrap();
    let State::Mines(ref s) = g.state else {
        panic!()
    };
    let mine = s.mine_for_test();
    g.apply(PuzzleInput::Reveal { cell: mine }).unwrap();
    assert_eq!(g.outcome(), Outcome::Lost);
    g.undo();
    assert_eq!(g.outcome(), Outcome::Playing);
    let PuzzleScene::Mines(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.deaths, 1);
    assert_eq!(s.cells[mine], MineCell::Covered);
    g.reset();
    let PuzzleScene::Mines(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.deaths, 0);
}
#[test]
fn invalid_every_game_bounds_and_givens_are_transactional() {
    for (kind, input) in [
        (
            PuzzleKind::Net,
            PuzzleInput::Rotate {
                cell: usize::MAX,
                clockwise: true,
            },
        ),
        (PuzzleKind::Mines, PuzzleInput::Reveal { cell: 81 }),
        (
            PuzzleKind::Solo,
            PuzzleInput::Digit {
                cell: 81,
                value: 255,
            },
        ),
        (PuzzleKind::Guess, PuzzleInput::Hold { slot: 4 }),
        (
            PuzzleKind::Map,
            PuzzleInput::Colour {
                region: 16,
                colour: 5,
            },
        ),
        (
            PuzzleKind::Untangle,
            PuzzleInput::Move {
                vertex: 0,
                x: 1001,
                y: 500,
            },
        ),
    ] {
        let mut g = PuzzleSession::new(kind, 9);
        let before = g.scene();
        assert!(g.apply(input).is_err());
        assert_eq!(g.scene(), before);
        assert!(!g.undo());
    }
    for kind in [PuzzleKind::Solo, PuzzleKind::Map] {
        let mut g = PuzzleSession::new(kind, 2);
        let input = match g.scene() {
            PuzzleScene::Solo(s) => PuzzleInput::Digit {
                cell: s.givens.iter().position(|&x| x).unwrap(),
                value: 0,
            },
            PuzzleScene::Map(s) => PuzzleInput::Colour {
                region: s.givens.iter().position(|&x| x).unwrap(),
                colour: 0,
            },
            _ => unreachable!(),
        };
        let before = g.scene();
        assert!(g.apply(input).is_err());
        assert_eq!(g.scene(), before);
    }
}
#[test]
fn recovered_filled_map_pencil_is_transactional() {
    let mut g = PuzzleSession::new(PuzzleKind::Map, 0);
    let PuzzleScene::Map(s) = g.scene() else {
        panic!()
    };
    let region = s.givens.iter().position(|&x| !x).unwrap();
    g.input(PuzzleInput::Colour { region, colour: 1 }).unwrap();
    let before = g.scene();
    let history = g.history.len();
    assert!(
        g.input(PuzzleInput::MapPencil { region, colour: 2 })
            .is_err()
    );
    assert_eq!(g.scene(), before);
    assert_eq!(g.history.len(), history);
    assert!(
        g.input(PuzzleInput::MapPencil { region, colour: 0 })
            .is_err()
    );
    assert_eq!(g.scene(), before);
    assert_eq!(g.history.len(), history);
}
#[test]
fn recovered_map_actual_default_dimensions() {
    let g = PuzzleSession::new(PuzzleKind::Map, 0);
    let PuzzleScene::Map(s) = g.scene() else {
        panic!()
    };
    assert_eq!((s.width, s.height, s.colours.len()), (20, 15, 30));
    assert_eq!(s.regions.len(), 300);
}
#[test]
fn recovered_net_half_turn_atomic_jumble_and_source_ui() {
    let mut g = PuzzleSession::new(PuzzleKind::Net, 7);
    let before = g.scene();
    assert!(g.input(PuzzleInput::HalfTurn { cell: 0 }).unwrap());
    assert_eq!(g.history.len(), 1);
    assert!(g.undo());
    assert_eq!(g.scene(), before);
    g.input(PuzzleInput::Lock { cell: 0 }).unwrap();
    let PuzzleScene::Net(s) = g.scene() else {
        panic!()
    };
    let locked = s.tiles[0];
    let input = PuzzleInput::Jumble { seed: 123 };
    g.input(input).unwrap();
    let jumbled = g.scene();
    let PuzzleScene::Net(s) = &jumbled else {
        panic!()
    };
    assert_eq!(s.tiles[0], locked);
    assert!(g.undo());
    g.input(input).unwrap();
    assert_eq!(g.scene(), jumbled);
    let history = g.history.len();
    g.input(PuzzleInput::NetSource {
        direction: Direction::North,
    })
    .unwrap();
    let PuzzleScene::Net(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.source, 7);
    assert_eq!(g.history.len(), history);
    g.undo();
    let PuzzleScene::Net(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.source, 7); // highlight is UI state, not puzzle undo
    g.reset();
    let PuzzleScene::Net(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.source, 12);
}
#[test]
fn recovered_net_source_wrap_locked_jumble_and_invalid_half_turn() {
    let mut g = PuzzleSession::new(PuzzleKind::Net, 5);
    for _ in 0..3 {
        g.input(PuzzleInput::NetSource {
            direction: Direction::North,
        })
        .unwrap();
    }
    let PuzzleScene::Net(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.source, 22);
    assert!(!g.can_undo());
    for cell in 0..25 {
        g.input(PuzzleInput::Lock { cell }).unwrap();
    }
    let before = g.scene();
    let history = g.history.len();
    assert!(!g.input(PuzzleInput::Jumble { seed: 9 }).unwrap());
    assert!(!g.input(PuzzleInput::HalfTurn { cell: 0 }).unwrap());
    assert_eq!(g.history.len(), history);
    assert_eq!(g.scene(), before);
    assert!(g.input(PuzzleInput::HalfTurn { cell: 25 }).is_err());
    assert_eq!(g.scene(), before);
}
