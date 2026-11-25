use harbor_workspace::games::screensaver::ScreensaverState;
use std::time::Duration;

#[test]
fn screensaver_requires_explicit_opt_in_and_exact_idle_threshold() {
    let mut saver = ScreensaverState::default();
    saver.tick(Duration::from_secs(86400));
    assert!(!saver.active());
    saver.configure(Some(Duration::from_secs(60)));
    saver.tick(Duration::from_secs(59));
    assert!(!saver.active());
    saver.tick(Duration::from_secs(1));
    assert!(saver.active());
    assert!(saver.on_input());
    assert!(!saver.active());
    saver.tick(Duration::from_secs(59));
    assert!(!saver.active());
}

#[test]
fn screensaver_disable_and_reconfigure_reset_idle() {
    let mut saver = ScreensaverState::default();
    saver.configure(Some(Duration::ZERO));
    saver.tick(Duration::MAX);
    assert!(!saver.active(), "zero timeout must never start a saver");
    saver.configure(Some(Duration::from_secs(1)));
    saver.tick(Duration::MAX);
    assert!(saver.active());
    saver.configure(None);
    assert!(!saver.active());
    assert!(!saver.on_input());
}

#[test]
fn recovered_thirteen_catalog_and_each_typed_session_make_real_moves() {
    use harbor_workspace::games::{
        GameId, GameInput, GameRegistry, GameScene, puzzles as p, terminal as t,
    };
    let names = GameRegistry::catalog()
        .iter()
        .map(|s| (s.label, s.selector))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            ("Soliterm", "soliterm"),
            ("Termwalk", "termwalk"),
            ("Ctris", "ctris"),
            ("Sudoku", "sudoku"),
            ("Nimesweeper", "nimesweeper"),
            ("Ninvaders", "ninvaders"),
            ("Greed", "greed"),
            ("Netwalk", "net"),
            ("Mines", "mines"),
            ("Solo", "solo"),
            ("Guess", "guess"),
            ("Map", "map"),
            ("Untangle", "untangle")
        ]
    );
    for spec in GameRegistry::catalog() {
        let mut game = GameRegistry::start(spec.id, 7);
        assert_eq!(game.scene(), GameRegistry::start(spec.id, 7).scene());
        let before = game.scene();
        let input = match game.scene() {
            GameScene::Terminal(t::TerminalScene::Soliterm(_)) => {
                GameInput::Terminal(t::TerminalInput::Transfer {
                    from: t::CardPile::Tableau(0),
                    to: t::CardPile::Reserve(0),
                    count: 1,
                })
            }
            GameScene::Terminal(t::TerminalScene::Termwalk(_)) => {
                GameInput::Terminal(t::TerminalInput::Rotate {
                    cell: 0,
                    clockwise: true,
                })
            }
            GameScene::Terminal(t::TerminalScene::Ctris(_)) => {
                GameInput::Terminal(t::TerminalInput::Move { dx: -1, dy: 0 })
            }
            GameScene::Terminal(t::TerminalScene::Sudoku(s)) => {
                let cell = s.fixed.iter().position(|&b| !b).unwrap();
                let value = (1..=9)
                    .find(|&n| {
                        (0..81).all(|i| {
                            i == cell
                                || s.cells[i] != n
                                || !(i / 9 == cell / 9
                                    || i % 9 == cell % 9
                                    || (i / 27 == cell / 27 && i % 9 / 3 == cell % 9 / 3))
                        })
                    })
                    .unwrap();
                GameInput::Terminal(t::TerminalInput::Digit { cell, value })
            }
            GameScene::Terminal(t::TerminalScene::Nimesweeper(_)) => {
                GameInput::Terminal(t::TerminalInput::Flag { cell: 0 })
            }
            GameScene::Terminal(t::TerminalScene::Ninvaders(_)) => {
                GameInput::Terminal(t::TerminalInput::Start)
            }
            GameScene::Terminal(t::TerminalScene::Greed(s)) => {
                let (dx, dy) = s.legal_directions[0];
                GameInput::Terminal(t::TerminalInput::Move { dx, dy })
            }
            GameScene::Puzzle(p::PuzzleScene::Net(_)) => {
                GameInput::Puzzle(p::PuzzleInput::Lock { cell: 0 })
            }
            GameScene::Puzzle(p::PuzzleScene::Mines(_)) => {
                GameInput::Puzzle(p::PuzzleInput::Flag { cell: 0 })
            }
            GameScene::Puzzle(p::PuzzleScene::Solo(s)) => {
                GameInput::Puzzle(p::PuzzleInput::Pencil {
                    cell: s.givens.iter().position(|&b| !b).unwrap(),
                    value: 1,
                })
            }
            GameScene::Puzzle(p::PuzzleScene::Guess(_)) => {
                GameInput::Puzzle(p::PuzzleInput::Peg { slot: 0, colour: 1 })
            }
            GameScene::Puzzle(p::PuzzleScene::Map(s)) => {
                GameInput::Puzzle(p::PuzzleInput::Colour {
                    region: s.givens.iter().position(|&b| !b).unwrap(),
                    colour: 1,
                })
            }
            GameScene::Puzzle(p::PuzzleScene::Untangle(s)) => {
                GameInput::Puzzle(p::PuzzleInput::Move {
                    vertex: 0,
                    x: (s.vertices[0].0 + 1).min(1000),
                    y: s.vertices[0].1,
                })
            }
        };
        assert_eq!(
            game.input(input),
            Ok(true),
            "{} actual typed move",
            spec.label
        );
        assert_ne!(game.scene(), before);
        assert!(game.can_undo());
        assert!(game.undo());
        assert_eq!(game.scene(), before);
        game.input(input).unwrap();
        game.reset();
        assert_eq!(game.scene(), before);
        assert!(!game.help().is_empty());
    }
    let mut guess = GameRegistry::start(GameId::Guess, 7);
    for _ in 0..10 {
        for slot in 0..4 {
            guess
                .input(GameInput::Puzzle(p::PuzzleInput::Peg { slot, colour: 1 }))
                .unwrap();
        }
        guess
            .input(GameInput::Puzzle(p::PuzzleInput::Submit))
            .unwrap();
    }
    assert_eq!(guess.outcome(), harbor_workspace::games::GameOutcome::Lost);
    assert!(guess.undo());
    assert_eq!(
        guess.outcome(),
        harbor_workspace::games::GameOutcome::Playing
    );
}

#[test]
fn puzzle_options_registry_roundtrip_assistance_and_redo() {
    use harbor_workspace::games::{GameId, GameInput, GameSession, puzzles as p};
    let c = p::PuzzleConfig::Guess(p::GuessConfig {
        colours: 8,
        pegs: 5,
        max_guesses: 12,
        allow_blank: true,
        allow_multiple: false,
    });
    let mut s = GameSession::new_puzzle(77, c).unwrap();
    assert_eq!(s.id(), GameId::Guess);
    assert_eq!(s.configuration(), Some(c));
    s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
        slot: 4,
        colour: 8,
    }))
    .unwrap();
    let before = s.scene();
    s.solve().unwrap();
    assert!(s.assisted());
    assert!(s.undo());
    assert_eq!(s.scene(), before);
    assert!(!s.assisted());
    assert!(s.can_redo());
    assert!(s.redo());
    assert!(s.assisted());
    s.reset();
    assert!(!s.assisted());
    assert!(!s.can_redo());
}

#[test]
fn puzzle_options_help_describes_actual_dynamic_rules() {
    use harbor_workspace::games::{GameSession, puzzles as p};
    let g = GameSession::new_puzzle(
        1,
        p::PuzzleConfig::Guess(p::GuessConfig {
            colours: 8,
            pegs: 5,
            max_guesses: 12,
            allow_blank: true,
            allow_multiple: false,
        }),
    )
    .unwrap();
    let h = g.help();
    assert!(h.contains("5 hidden pegs"));
    assert!(h.contains("8 colours"));
    assert!(h.contains("12 guesses"));
    assert!(h.contains("different colour"));
    assert!(h.contains("Blank guesses allowed"));
    assert!(!h.contains("budget"));
    let g = GameSession::new_puzzle(
        1,
        p::PuzzleConfig::Solo(p::SoloConfig {
            block_width: 4,
            block_height: 4,
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(g.help().contains("1–16"));
    assert!(g.help().contains("4×4 box"));
}
