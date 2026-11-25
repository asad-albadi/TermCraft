#![allow(dead_code)]
#[path = "/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/b06-puzzles/component/src/games/puzzles/mod.rs"]
mod puzzles;
use puzzles::*;
#[test]
fn filled_map_region_rejects_pencil_transactionally() {
    let mut g = PuzzleSession::new(PuzzleKind::Map, 0);
    let PuzzleScene::Map(s) = g.scene() else { panic!() };
    let region = s.givens.iter().position(|&v| !v).unwrap();
    g.input(PuzzleInput::Colour { region, colour: 1 }).unwrap();
    let before = g.scene();
    // Historical map.c 2521-2524 rejects stipples on any filled region.
    assert!(g.input(PuzzleInput::MapPencil { region, colour: 2 }).is_err());
    assert_eq!(g.scene(), before);
}
