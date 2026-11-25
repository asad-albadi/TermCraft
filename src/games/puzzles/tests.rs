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
#[test]
fn redo_restores_moves_and_only_changed_branch_clears_it() {
    let mut g = PuzzleSession::new(PuzzleKind::Guess, 5);
    let start = g.scene();
    g.input(PuzzleInput::Peg { slot: 0, colour: 2 }).unwrap();
    let changed = g.scene();
    assert!(g.undo());
    assert!(g.can_redo());
    assert!(
        g.input(PuzzleInput::Peg {
            slot: 99,
            colour: 2
        })
        .is_err()
    );
    assert!(!g.input(PuzzleInput::Peg { slot: 0, colour: 0 }).unwrap());
    assert!(g.redo());
    assert_eq!(g.scene(), changed);
    assert!(g.undo());
    assert_eq!(g.scene(), start);
    g.input(PuzzleInput::Peg { slot: 1, colour: 3 }).unwrap();
    assert!(!g.can_redo());
    assert!(!g.redo());
    g.undo();
    g.reset();
    assert!(!g.can_redo());
}
#[test]
fn explicit_solve_is_atomic_assisted_undoable_and_guess_gives_up() {
    for kind in PuzzleKind::ALL {
        let mut g = PuzzleSession::new(kind, 9);
        if kind == PuzzleKind::Mines {
            let before = g.scene();
            assert!(g.solve().is_err());
            assert_eq!(g.scene(), before);
            assert!(!g.can_undo());
            g.input(PuzzleInput::Reveal { cell: 40 }).unwrap();
        }
        let before = g.scene();
        let history = g.history.len();
        assert!(g.solve().unwrap());
        assert!(g.assisted());
        assert_eq!(
            g.outcome(),
            if kind == PuzzleKind::Guess {
                Outcome::Lost
            } else {
                Outcome::Won
            }
        );
        assert_eq!(g.history.len(), history + 1);
        let solved = g.scene();
        assert!(g.undo());
        assert_eq!(g.scene(), before);
        assert!(!g.assisted());
        assert!(g.redo());
        assert_eq!(g.scene(), solved);
        assert!(g.assisted());
        g.reset();
        assert!(!g.assisted());
    }
}
#[test]
fn typed_configuration_reset_and_source_metadata_are_honest() {
    let cfg = PuzzleConfig::Net(NetConfig {
        width: 7,
        height: 5,
        wrapping: true,
        barrier_percent: 25,
        unique: false,
    });
    let mut g = PuzzleSession::new_config(4, cfg).unwrap();
    let start = g.scene();
    assert_eq!(g.configuration(), cfg);
    g.input(PuzzleInput::NetSource {
        direction: Direction::North,
    })
    .unwrap();
    let PuzzleScene::Net(n) = g.scene() else {
        panic!()
    };
    assert_eq!(n.source, 10);
    g.input(PuzzleInput::Lock { cell: 0 }).unwrap();
    g.reset();
    assert_eq!(g.scene(), start);
    assert_eq!(g.configuration(), cfg);
    for kind in PuzzleKind::ALL {
        assert!(kind.presets().iter().any(|p| p.configuration.is_some()));
        for preset in kind.presets() {
            if let Some(cfg) = preset.configuration {
                assert_eq!(cfg.kind(), kind);
                cfg.validate().unwrap();
            } else {
                assert!(!preset.note.is_empty());
            }
        }
    }
    // Historical c,r labels count block rows/columns; solo.c:3611 uses
    // (y/c)*c+(x/r), so the physical box width is r and height is c.
    for (name, width, height) in [("2x3 Basic", 3, 2), ("3x4 Basic", 4, 3)] {
        let preset = PuzzleKind::Solo
            .presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap();
        let Some(PuzzleConfig::Solo(c)) = preset.configuration else {
            panic!()
        };
        assert_eq!((c.block_width, c.block_height), (width, height));
    }
    let cfg = PuzzleConfig::Guess(GuessConfig {
        colours: 8,
        pegs: 5,
        max_guesses: 12,
        allow_blank: false,
        allow_multiple: true,
    });
    let g = PuzzleSession::new_config(1, cfg).unwrap();
    assert!(g.help().contains("5"));
    assert!(g.help().contains("8"));
    assert!(PuzzleSession::new_config(0, PuzzleConfig::Untangle { points: usize::MAX }).is_err());
    for config in [
        PuzzleConfig::Net(NetConfig {
            width: usize::MAX,
            ..NetConfig::default()
        }),
        PuzzleConfig::Mines(MinesConfig {
            mines: usize::MAX,
            ..MinesConfig::default()
        }),
        PuzzleConfig::Solo(SoloConfig {
            block_width: usize::MAX,
            block_height: usize::MAX,
            ..SoloConfig::default()
        }),
        PuzzleConfig::Guess(GuessConfig {
            pegs: usize::MAX,
            ..GuessConfig::default()
        }),
        PuzzleConfig::Map(MapConfig {
            regions: usize::MAX,
            ..MapConfig::default()
        }),
    ] {
        assert!(PuzzleSession::new_config(0, config).is_err());
        assert!(config.help().starts_with("Invalid puzzle configuration:"));
    }
}
#[test]
fn selectable_stock_presets_smoke_and_disclose_generation_failure() {
    for kind in PuzzleKind::ALL {
        for preset in kind.presets() {
            let Some(config) = preset.configuration else {
                continue;
            };
            let result = PuzzleSession::new_config(2, config);
            match result {
                Ok(mut session) => {
                    if kind == PuzzleKind::Mines {
                        let PuzzleScene::Mines(scene) = session.scene() else {
                            panic!()
                        };
                        let before = session.scene();
                        match session.input(PuzzleInput::Reveal {
                            cell: scene.width * scene.height / 2,
                        }) {
                            Ok(_) => {
                                session.solve().unwrap();
                                assert_eq!(session.outcome(), Outcome::Won);
                            }
                            Err(message) => {
                                assert!(message.contains("budget exhausted"));
                                assert_eq!(session.scene(), before);
                                assert!(!session.can_undo());
                                println!("{}: explicit first-reveal exhaustion", preset.name);
                            }
                        }
                    } else {
                        let start = session.scene();
                        session.solve().unwrap();
                        assert_ne!(session.outcome(), Outcome::Playing);
                        session.undo();
                        assert_eq!(session.scene(), start);
                    }
                }
                Err(message) => {
                    assert!(message.contains("budget exhausted"));
                    println!("{}: explicit constructor exhaustion", preset.name);
                }
            }
        }
    }
}
#[test]
fn mines_explicit_solve_after_death_reveals_corrections_and_undo_hides_them() {
    let mut g = PuzzleSession::new(PuzzleKind::Mines, 5);
    g.input(PuzzleInput::Reveal { cell: 40 }).unwrap();
    let State::Mines(ref m) = g.state else {
        panic!()
    };
    let mine = m.mine_for_test();
    g.input(PuzzleInput::Reveal { cell: mine }).unwrap();
    let before = g.scene();
    let PuzzleScene::Mines(s) = &before else {
        panic!()
    };
    assert!(s.solution.is_none());
    assert!(g.solve().unwrap());
    assert_eq!(g.outcome(), Outcome::Lost);
    let PuzzleScene::Mines(s) = g.scene() else {
        panic!()
    };
    assert_eq!(s.solution.unwrap().iter().filter(|&&v| v).count(), 10);
    assert_eq!(s.deaths, 1);
    assert!(g.undo());
    assert_eq!(g.scene(), before);
    assert!(!g.assisted());
}
#[test]
fn option_sample_reports_actual_certificates_and_fallbacks() {
    for kind in PuzzleKind::ALL {
        for preset in kind.presets() {
            let Some(config) = preset.configuration else {
                continue;
            };
            let (mut success, mut errors, mut fallback) = (0, 0, 0);
            for seed in 0..8 {
                let Ok(mut g) = PuzzleSession::new_config(seed, config) else {
                    errors += 1;
                    continue;
                };
                if kind == PuzzleKind::Mines {
                    let PuzzleScene::Mines(s) = g.scene() else {
                        panic!()
                    };
                    if g.input(PuzzleInput::Reveal {
                        cell: s.width * s.height / 2,
                    })
                    .is_err()
                    {
                        errors += 1;
                        continue;
                    }
                }
                match g.scene() {
                    PuzzleScene::Solo(s) => {
                        fallback += usize::from(s.certificate.easier_fallback);
                        assert_eq!(
                            s.certificate.requested,
                            if let PuzzleConfig::Solo(c) = config {
                                c.difficulty
                            } else if let PuzzleConfig::SoloVariant(c) = config {
                                c.base.difficulty
                            } else {
                                panic!()
                            }
                        );
                    }
                    PuzzleScene::Mines(s) => {
                        fallback += usize::from(s.generation == MinesGeneration::CertifiedFallback)
                    }
                    PuzzleScene::Map(s) => {
                        fallback += usize::from(s.generation == MapGeneration::CertifiedFallback)
                    }
                    _ => {}
                }
                success += 1;
            }
            println!(
                "sample {}: {} success, {} explicit exhaustion, {} disclosed fallback",
                preset.name, success, errors, fallback
            );
            assert!(success > 0, "{} never generated in the sample", preset.name);
        }
    }
}

// Historical net.c:1746–1825 known-answer Solve sets LOCKED on every tile.
fn assert_net_solve_locks_history(mut game: PuzzleSession) {
    let configuration = game.configuration();
    let initial = game.scene();
    let PuzzleScene::Net(initial_net) = &initial else {
        panic!()
    };
    let count = initial_net.tiles.len();
    assert!(initial_net.locked.iter().all(|&locked| !locked));
    game.input(PuzzleInput::Lock { cell: 0 }).unwrap();
    if count != 25 {
        game.input(PuzzleInput::Lock { cell: count - 1 }).unwrap();
    }
    let partial = game.scene();
    let history_before = game.history.len();
    assert!(game.solve().unwrap());
    let solved = game.scene();
    let PuzzleScene::Net(net) = &solved else {
        panic!()
    };
    assert_eq!(game.outcome(), Outcome::Won);
    assert_eq!(net.locked.iter().filter(|&&locked| locked).count(), count);
    assert_eq!(game.history.len(), history_before + 1);
    assert!(game.assisted());
    assert!(game.undo());
    assert_eq!(game.scene(), partial);
    assert_eq!(game.history.len(), history_before);
    assert!(!game.assisted());
    assert!(game.can_redo());
    assert!(game.redo());
    assert_eq!(game.scene(), solved);
    assert_eq!(game.history.len(), history_before + 1);
    assert!(game.assisted());
    game.reset();
    assert_eq!(game.scene(), initial);
    assert_eq!(game.configuration(), configuration);
    assert!(!game.assisted());
    assert!(!game.can_undo());
    assert!(!game.can_redo());
}
#[test]
fn net_solve_locks_every_default_tile_atomically_through_history() {
    assert_net_solve_locks_history(PuzzleSession::new(PuzzleKind::Net, 9));
}
#[test]
fn net_solve_locks_every_configured_tile_atomically_through_history() {
    let configuration = PuzzleConfig::Net(NetConfig {
        width: 7,
        height: 5,
        wrapping: true,
        barrier_percent: 25,
        unique: false,
    });
    let game = PuzzleSession::new_config(9, configuration).unwrap();
    assert_eq!(game.configuration(), configuration);
    assert_net_solve_locks_history(game);
}

#[test]
fn unflagged_variant_has_truthful_regular_metadata_and_topology() {
    let base = SoloConfig::default();
    let variant = SoloVariantConfig {
        base,
        jigsaw: false,
        killer: false,
    };
    let configuration = PuzzleConfig::SoloVariant(variant);
    assert_eq!(variant.validate(), Ok(()));
    assert_eq!(configuration.validate(), Ok(()));
    let help = configuration.help();
    assert!(help.starts_with("9×9 Regular Solo"), "{help}");
    assert!(!help.contains("Killer"));
    assert!(!help.contains("Jigsaw"));
    let mut session = PuzzleSession::new_config(0, configuration).unwrap();
    assert_eq!(session.configuration(), configuration);
    assert_eq!(session.kind(), PuzzleKind::Solo);
    assert_eq!(session.help(), help);
    let PuzzleScene::Solo(scene) = session.scene() else {
        panic!()
    };
    assert_eq!(
        (
            scene.width,
            scene.height,
            scene.block_width,
            scene.block_height
        ),
        (9, 9, 3, 3)
    );
    assert!(!scene.diagonal);
    assert!(scene.cages.is_empty());
    assert_eq!(scene.certificate.requested, SoloDifficulty::Trivial);
    assert_eq!(scene.certificate.achieved, SoloDifficulty::Trivial);
    assert!(!scene.certificate.killer_intersection_required);
    for i in 0..81 {
        assert_eq!(scene.regions[i], i / 27 * 3 + i % 9 / 3);
    }
    let initial = session.scene();
    session.solve().unwrap();
    assert_eq!(session.outcome(), Outcome::Won);
    assert!(session.assisted());
    assert!(session.undo());
    assert_eq!(session.scene(), initial);
    session.reset();
    assert_eq!(session.scene(), initial);
    // Existing ordinary/default APIs retain their explicit ordinary configuration.
    for default in [
        PuzzleSession::new(PuzzleKind::Solo, 0),
        PuzzleSession::new_config(0, PuzzleConfig::Solo(base)).unwrap(),
    ] {
        assert_eq!(default.configuration(), PuzzleConfig::Solo(base));
        assert!(!default.help().contains("Killer"));
        assert!(!default.help().contains("Jigsaw"));
        let PuzzleScene::Solo(scene) = default.scene() else {
            panic!()
        };
        assert!(scene.cages.is_empty());
        for i in 0..81 {
            assert_eq!(scene.regions[i], i / 27 * 3 + i % 9 / 3);
        }
    }
}
