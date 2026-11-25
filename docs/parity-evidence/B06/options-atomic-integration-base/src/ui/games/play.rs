use super::*;
use harbor_workspace::games::{
    GameId, GameInput, GameOutcome, GameRegistry, GameScene, GameSession, puzzles as p,
    terminal as t,
};
pub(in crate::ui) struct PlayState {
    launcher: bool,
    suspended: bool,
    pub(in crate::ui) session: Option<GameSession>,
    selected: usize,
    colour: u8,
    pencil: bool,
    help: bool,
    message: String,
    close_owner: Option<ViewportId>,
    tick: Option<Instant>,
    source: Option<t::CardPile>,
    count: usize,
    variant: usize,
    passes: u8,
    preset: usize,
    portrait: bool,
    walk_size: usize,
    dragging: Option<usize>,
    pub(in crate::ui) board: Option<Rect>,
}
impl Default for PlayState {
    fn default() -> Self {
        Self {
            launcher: false,
            suspended: false,
            session: None,
            selected: 0,
            colour: 1,
            pencil: false,
            help: false,
            message: String::new(),
            close_owner: None,
            tick: None,
            source: None,
            count: 1,
            variant: 0,
            passes: 3,
            preset: 1,
            portrait: false,
            walk_size: 5,
            dragging: None,
            board: None,
        }
    }
}
impl PlayState {
    pub(in crate::ui) fn open(&mut self) {
        self.launcher = true;
        self.suspended = false;
    }
    pub(in crate::ui) fn is_open(&self) -> bool {
        !self.suspended && (self.launcher || self.session.is_some())
    }
    fn start(&mut self, id: GameId, seed: u64) {
        self.session = Some(match id {
            GameId::Soliterm => GameSession::new_solitaire(
                seed,
                if self.variant == 1 {
                    t::SolitaireVariant::Klondike
                } else {
                    t::SolitaireVariant::Freecell
                },
                self.passes.max(1),
            ),
            GameId::Nimesweeper => GameSession::new_mines(
                seed,
                [
                    t::MinePreset::Newbie,
                    t::MinePreset::Easy,
                    t::MinePreset::Medium,
                    t::MinePreset::Hard,
                ][self.preset.min(3)],
            ),
            GameId::Termwalk => GameSession::new_termwalk(
                seed,
                if self.walk_size == 0 {
                    5
                } else {
                    self.walk_size
                },
            )
            .expect("bounded odd UI grid"),
            GameId::Map => GameSession::new_map(
                seed,
                if self.portrait {
                    p::MapPreset::Portrait
                } else {
                    p::MapPreset::Default
                },
            ),
            _ => GameRegistry::start(id, seed),
        });
        self.launcher = false;
        self.suspended = false;
        self.selected = 0;
        self.source = None;
        self.count = 1;
        self.message.clear();
        self.tick = None;
        self.dragging = None;
        self.board = None;
    }
    fn apply(&mut self, i: GameInput) {
        if let Some(s) = &mut self.session {
            match s.input(i) {
                Ok(_) => self.message.clear(),
                Err(e) => self.message = e.into(),
            }
        }
    }
    fn puzzle(&mut self, i: p::PuzzleInput) {
        self.apply(GameInput::Puzzle(i))
    }
    fn terminal(&mut self, i: t::TerminalInput) {
        self.apply(GameInput::Terminal(i))
    }
    fn dimensions(&self) -> (usize, usize) {
        match self.session.as_ref().map(GameSession::scene) {
            Some(GameScene::Puzzle(p::PuzzleScene::Net(s))) => (s.width, s.height),
            Some(GameScene::Puzzle(p::PuzzleScene::Mines(s))) => (s.width, s.height),
            Some(GameScene::Puzzle(p::PuzzleScene::Solo(_))) => (9, 9),
            Some(GameScene::Puzzle(p::PuzzleScene::Map(s))) => (s.width, s.height),
            Some(GameScene::Puzzle(p::PuzzleScene::Untangle(s))) => (s.vertices.len(), 1),
            Some(GameScene::Terminal(t::TerminalScene::Soliterm(s))) => (9 + s.tableau.len(), 1),
            Some(GameScene::Terminal(t::TerminalScene::Termwalk(s))) => (s.width, s.height),
            Some(GameScene::Terminal(t::TerminalScene::Sudoku(_))) => (9, 9),
            Some(GameScene::Terminal(t::TerminalScene::Nimesweeper(s))) => (s.width, s.height),
            Some(GameScene::Terminal(t::TerminalScene::Greed(s))) => (s.width, s.height),
            _ => (4, 1),
        }
    }
    fn key(&mut self, key: Key, m: Modifiers) {
        let Some(id) = self.session.as_ref().map(GameSession::id) else {
            return;
        };
        let (w, h) = self.dimensions();
        self.selected = self.selected.min(w * h - 1);
        let direction = match key {
            Key::ArrowLeft => Some((-1, 0)),
            Key::ArrowRight => Some((1, 0)),
            Key::ArrowUp => Some((0, -1)),
            Key::ArrowDown => Some((0, 1)),
            _ => None,
        };
        if id == GameId::Untangle {
            if let Some(GameScene::Puzzle(p::PuzzleScene::Untangle(s))) =
                self.session.as_ref().map(GameSession::scene)
            {
                self.selected = self.selected.min(s.vertices.len() - 1);
                if key == Key::Tab {
                    self.selected = (self.selected + 1) % s.vertices.len();
                    return;
                }
                if let Some((dx, dy)) = direction {
                    let (x, y) = s.vertices[self.selected];
                    let step = if m.shift { 5 } else { 20 };
                    self.puzzle(p::PuzzleInput::Move {
                        vertex: self.selected,
                        x: (x + dx as i32 * step).clamp(0, 1000),
                        y: (y + dy as i32 * step).clamp(0, 1000),
                    });
                    return;
                }
            }
        }
        if let Some((dx, dy)) = direction {
            if matches!(id, GameId::Ctris | GameId::Ninvaders | GameId::Greed) {
                if id == GameId::Ctris && dy < 0 {
                    self.terminal(t::TerminalInput::Rotate {
                        cell: 0,
                        clockwise: true,
                    });
                } else {
                    self.terminal(t::TerminalInput::Move { dx, dy });
                }
                return;
            }
            if id == GameId::Netwalk && m.ctrl {
                self.puzzle(p::PuzzleInput::NetSource {
                    direction: match (dx, dy) {
                        (-1, 0) => p::Direction::West,
                        (1, 0) => p::Direction::East,
                        (0, -1) => p::Direction::North,
                        _ => p::Direction::South,
                    },
                });
                return;
            }
            let x = (self.selected % w) as i32;
            let y = (self.selected / w) as i32;
            self.selected = (y + dy as i32).clamp(0, h as i32 - 1) as usize * w
                + (x + dx as i32).clamp(0, w as i32 - 1) as usize;
            return;
        }
        if m.ctrl && key == Key::Z {
            if let Some(s) = &mut self.session {
                s.undo();
            }
            return;
        }
        let digit = match key {
            Key::Num0 => Some(0),
            Key::Num1 => Some(1),
            Key::Num2 => Some(2),
            Key::Num3 => Some(3),
            Key::Num4 => Some(4),
            Key::Num5 => Some(5),
            Key::Num6 => Some(6),
            Key::Num7 => Some(7),
            Key::Num8 => Some(8),
            Key::Num9 => Some(9),
            Key::Delete | Key::Backspace => Some(0),
            _ => None,
        };
        if let Some(n) = digit {
            if id == GameId::Greed {
                let d = match n {
                    1 => Some((-1, 1)),
                    2 => Some((0, 1)),
                    3 => Some((1, 1)),
                    4 => Some((-1, 0)),
                    6 => Some((1, 0)),
                    7 => Some((-1, -1)),
                    8 => Some((0, -1)),
                    9 => Some((1, -1)),
                    _ => None,
                };
                if let Some((dx, dy)) = d {
                    self.terminal(t::TerminalInput::Move { dx, dy });
                }
            } else {
                self.number(n);
            }
            return;
        }
        match (id, key) {
            (GameId::Netwalk, Key::A) => self.puzzle(p::PuzzleInput::Rotate {
                cell: self.selected,
                clockwise: false,
            }),
            (GameId::Netwalk, Key::D) => self.puzzle(p::PuzzleInput::Rotate {
                cell: self.selected,
                clockwise: true,
            }),
            (GameId::Netwalk, Key::S | Key::Space) => self.puzzle(p::PuzzleInput::Lock {
                cell: self.selected,
            }),
            (GameId::Netwalk, Key::F) => self.puzzle(p::PuzzleInput::HalfTurn {
                cell: self.selected,
            }),
            (GameId::Netwalk, Key::J) => {
                let seed = self.session.as_ref().unwrap().seed().wrapping_add(1);
                self.puzzle(p::PuzzleInput::Jumble { seed });
            }
            (GameId::Termwalk, Key::Space | Key::D) => self.terminal(t::TerminalInput::Rotate {
                cell: self.selected,
                clockwise: true,
            }),
            (GameId::Termwalk, Key::A) => self.terminal(t::TerminalInput::Rotate {
                cell: self.selected,
                clockwise: false,
            }),
            (GameId::Mines, Key::Space | Key::F) => self.puzzle(p::PuzzleInput::Flag {
                cell: self.selected,
            }),
            (GameId::Mines, Key::Enter) => self.puzzle(p::PuzzleInput::Reveal {
                cell: self.selected,
            }),
            (GameId::Nimesweeper, Key::F) => self.terminal(t::TerminalInput::Flag {
                cell: self.selected,
            }),
            (GameId::Nimesweeper, Key::Space) => self.terminal(t::TerminalInput::Reveal {
                cell: self.selected,
            }),
            (GameId::Nimesweeper, Key::Enter) => self.terminal(t::TerminalInput::Chord {
                cell: self.selected,
            }),
            (GameId::Ctris, Key::Space) => self.terminal(t::TerminalInput::Drop),
            (GameId::Ctris | GameId::Ninvaders | GameId::Nimesweeper, Key::P) => {
                self.terminal(t::TerminalInput::Pause)
            }
            (GameId::Ninvaders, Key::Space) => {
                self.terminal(t::TerminalInput::Start);
                self.terminal(t::TerminalInput::Fire);
            }
            (GameId::Soliterm, Key::Space | Key::Enter) => {
                let pile = match self.selected {
                    0..=3 => t::CardPile::Reserve(self.selected),
                    4..=7 => t::CardPile::Foundation(self.selected - 4),
                    8 => t::CardPile::Waste,
                    n => t::CardPile::Tableau(n - 9),
                };
                pile_select(self, pile);
            }
            (GameId::Soliterm, Key::D) => self.terminal(t::TerminalInput::Draw),
            (GameId::Guess, Key::Enter) => self.puzzle(p::PuzzleInput::Submit),
            (GameId::Guess, Key::Space) => self.puzzle(p::PuzzleInput::Hold {
                slot: self.selected.min(3),
            }),
            (GameId::Greed, k) => {
                let d = match k {
                    Key::H => Some((-1, 0)),
                    Key::J => Some((0, 1)),
                    Key::K => Some((0, -1)),
                    Key::L => Some((1, 0)),
                    Key::Y => Some((-1, -1)),
                    Key::U => Some((1, -1)),
                    Key::B => Some((-1, 1)),
                    Key::N => Some((1, 1)),
                    _ => None,
                };
                if let Some((dx, dy)) = d {
                    self.terminal(t::TerminalInput::Move { dx, dy });
                }
            }
            _ => {}
        }
    }
    fn number(&mut self, n: u8) {
        let Some(id) = self.session.as_ref().map(GameSession::id) else {
            return;
        };
        match id {
            GameId::Solo => self.puzzle(if self.pencil {
                p::PuzzleInput::Pencil {
                    cell: self.selected,
                    value: n,
                }
            } else {
                p::PuzzleInput::Digit {
                    cell: self.selected,
                    value: n,
                }
            }),
            GameId::Sudoku => self.terminal(t::TerminalInput::Digit {
                cell: self.selected,
                value: n,
            }),
            GameId::Guess => self.puzzle(p::PuzzleInput::Peg {
                slot: self.selected.min(3),
                colour: n,
            }),
            GameId::Map => {
                if let Some(GameScene::Puzzle(p::PuzzleScene::Map(s))) =
                    self.session.as_ref().map(GameSession::scene)
                {
                    if let Some(&region) = s.regions.get(self.selected) {
                        self.puzzle(if self.pencil {
                            p::PuzzleInput::MapPencil { region, colour: n }
                        } else {
                            p::PuzzleInput::Colour { region, colour: n }
                        })
                    }
                }
            }
            _ => {}
        }
    }
}
fn held_game_viewport(ctx: &Context) -> Option<ViewportId> {
    let current = ctx.viewport_id();
    let live = ctx.input(|i| {
        i.raw
            .viewports
            .iter()
            .filter_map(|(&id, info)| {
                if info.focused == Some(false) || (id == current && !i.raw.focused) {
                    None
                } else {
                    Some(id)
                }
            })
            .collect::<Vec<_>>()
    });
    live.into_iter().find(|&id| {
        ctx.input_for(id, |i| {
            !i.keys_down.is_empty() || i.modifiers.any() || i.pointer.any_down()
        })
    })
}
impl Workspace {
    pub(in crate::ui) fn games_menu(&mut self, ui: &mut Ui) {
        for spec in GameRegistry::catalog() {
            if ui.button(spec.label).clicked() {
                self.games.play.start(spec.id, 1);
                ui.close();
            }
        }
        if ui.button("Game library…").clicked() {
            self.games.play.open();
            ui.close();
        }
    }
    pub(in crate::ui) fn games_frame(&mut self, ui: &mut Ui) -> bool {
        let current = ui.ctx().viewport_id();
        let play = &mut self.games.play;
        if play.is_open() && ui.ctx().input(|i| i.viewport().close_requested()) {
            play.suspended = true;
            play.close_owner = held_game_viewport(ui.ctx());
            ui.ctx().input_mut(|i| i.events.clear());
        }
        if play.suspended && play.close_owner.is_none() {
            return false;
        }
        if let Some(owner) = play.close_owner {
            let missing = ui.ctx().input(|i| !i.raw.viewports.contains_key(&owner));
            let unfocused = ui.ctx().input(|i| {
                (owner == current && !i.raw.focused)
                    || i.raw
                        .viewports
                        .get(&owner)
                        .is_some_and(|v| v.focused == Some(false))
            });
            let canceled = ui.ctx().input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        Event::PointerGone
                            | Event::Touch {
                                phase: TouchPhase::Cancel,
                                ..
                            }
                            | Event::WindowFocused(false)
                    )
                })
            });
            if missing || unfocused {
                play.close_owner = None;
                if owner == current {
                    ui.ctx().input_mut(|i| {
                        i.pointer = Default::default();
                        i.keys_down.clear();
                        i.modifiers = Default::default();
                    });
                }
            } else if owner == current {
                if canceled {
                    ui.ctx().input_mut(|i| {
                        i.pointer = Default::default();
                    });
                }
                if !ui
                    .ctx()
                    .input(|i| !i.keys_down.is_empty() || i.modifiers.any() || i.pointer.any_down())
                {
                    play.close_owner = None;
                }
            }
            if play.close_owner.is_none() {
                play.close_owner = held_game_viewport(ui.ctx());
            }
            ui.ctx().input_mut(|i| i.events.clear());
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            return true;
        }
        if !play.is_open() {
            return false;
        }
        let events = ui.ctx().input(|i| i.events.clone());
        if events.iter().any(|e| {
            matches!(
                e,
                Event::PointerGone
                    | Event::Touch {
                        phase: TouchPhase::Cancel,
                        ..
                    }
                    | Event::WindowFocused(false)
            )
        }) {
            play.dragging = None;
            ui.ctx().input_mut(|i| {
                i.pointer = Default::default();
            });
        }
        if current != ViewportId::ROOT {
            ui.label("A game is open in the main window. Your sessions are still running.");
            ui.ctx().input_mut(|i| i.events.clear());
            return true;
        }
        // Only the foreground game owns these keys. No global shortcut processing follows.
        let mut close = false;
        for event in events {
            if let Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if key == Key::Escape {
                    close = true
                } else if !play.launcher {
                    play.key(key, modifiers)
                }
            }
        }
        Frame::new().fill(content(ui)).inner_margin(16.).show(ui,|ui|{
            ui.horizontal(|ui|{
                ui.image((self.logo.id(),vec2(28.,28.)));
                ui.heading(play.session.as_ref().filter(|_|!play.launcher).map_or("Games",|s|s.spec().label));
                if ui.button("Close (Esc)").clicked(){close=true;}
                if play.session.is_some()&&ui.button(if play.launcher{"Resume game"}else{"Library"}).clicked(){play.launcher=!play.launcher;}
            });
            if play.launcher {
                ui.label("Choose a game. Terminal sessions continue in the background.");
                for spec in GameRegistry::catalog(){if ui.button(spec.label).clicked(){play.start(spec.id,1);}}
            }else if let Some(s)=&play.session {
                let(id,seed,outcome)=(s.id(),s.seed(),s.outcome());
                ui.horizontal(|ui|{
                    if ui.button("New game").clicked(){play.start(id,seed.wrapping_add(1));}
                    if ui.button("Reset").clicked(){play.session.as_mut().unwrap().reset();play.message.clear();}
                    if ui.add_enabled(play.session.as_ref().unwrap().can_undo(),egui::Button::new("Undo")).clicked(){play.session.as_mut().unwrap().undo();}
                    ui.toggle_value(&mut play.help,"Help");
                    ui.label(format!("Seed {seed} · {outcome:?}"));
                });
                ui.horizontal_wrapped(|ui|{match id {
                    GameId::Soliterm=>{ui.label("New game variant:");ui.selectable_value(&mut play.variant,0,"Freecell");ui.selectable_value(&mut play.variant,1,"Klondike");ui.add(egui::Slider::new(&mut play.passes,1..=20).text("passes"));},
                    GameId::Nimesweeper=>{ui.label("New game preset:");for(i,n)in ["Newbie","Easy","Medium","Hard"].iter().enumerate(){ui.selectable_value(&mut play.preset,i,*n);}},
                    GameId::Termwalk=>{ui.label("New game size:");for size in [3,5,7,9,11,13,15]{ui.selectable_value(&mut play.walk_size,size,format!("{size}×{size}"));}},
                    GameId::Map=>{ui.checkbox(&mut play.portrait,"Portrait on new game");},_=>{}}});
                if play.help {ui.label(play.session.as_ref().unwrap().help());ui.label("Arrows select a square. Escape closes; Ctrl+Z undoes. Untangle: Tab selects a point, arrows move it. Cards: arrows select a pile, Space selects source/destination, D draws. Buttons below expose each move. Variant selections apply to New game.");ui.label("Independent rules and controls. Original runtime behavior and some historical variants remain unverified.");}
                if matches!(id,GameId::Solo|GameId::Map){ui.checkbox(&mut play.pencil,"Pencil marks");}
                if matches!(id,GameId::Solo|GameId::Sudoku|GameId::Guess|GameId::Map){ui.horizontal(|ui|{let max=if id==GameId::Guess{6}else if id==GameId::Map{4}else{9};for n in 0..=max{if ui.button(n.to_string()).clicked(){play.number(n)}}});}
                controls(ui,play,id);
                ScrollArea::both().show(ui,|ui|{draw_scene(ui,play);});
                if outcome!=GameOutcome::Playing{ui.strong("Game finished — Undo, Reset or New game to continue.");}
                if !play.message.is_empty(){ui.label(RichText::new(&play.message).color(Color32::from_rgb(215,120,55)));}
            }
        });
        if !play.launcher
            && play.session.as_ref().is_some_and(|s| {
                matches!(s.id(), GameId::Ctris | GameId::Ninvaders)
                    && s.outcome() == GameOutcome::Playing
            })
        {
            let now = Instant::now();
            let delay = if play.session.as_ref().unwrap().id() == GameId::Ctris {
                Duration::from_millis(500)
            } else {
                Duration::from_millis(100)
            };
            if play
                .tick
                .is_some_and(|last| now.saturating_duration_since(last) >= delay)
            {
                play.terminal(t::TerminalInput::Tick);
                play.tick = Some(now);
            } else if play.tick.is_none() {
                play.tick = Some(now);
            }
            ui.ctx().request_repaint_after(delay);
        } else {
            play.tick = None;
        }
        if close {
            play.session = None;
            play.launcher = false;
            play.tick = None;
            play.dragging = None;
            play.close_owner = held_game_viewport(ui.ctx());
        }
        ui.ctx().input_mut(|i| i.events.clear());
        // Polling remains bounded and active while terminal rendering is suspended.
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        true
    }
}
fn controls(ui: &mut Ui, play: &mut PlayState, id: GameId) {
    ui.horizontal_wrapped(|ui| match id {
        GameId::Netwalk => {
            for (name, cmd) in [
                (
                    "↶",
                    p::PuzzleInput::Rotate {
                        cell: play.selected,
                        clockwise: false,
                    },
                ),
                (
                    "↷",
                    p::PuzzleInput::Rotate {
                        cell: play.selected,
                        clockwise: true,
                    },
                ),
                (
                    "Half-turn",
                    p::PuzzleInput::HalfTurn {
                        cell: play.selected,
                    },
                ),
                (
                    "Lock",
                    p::PuzzleInput::Lock {
                        cell: play.selected,
                    },
                ),
                (
                    "Jumble",
                    p::PuzzleInput::Jumble {
                        seed: play.session.as_ref().unwrap().seed().wrapping_add(1),
                    },
                ),
            ] {
                if ui.button(name).clicked() {
                    play.puzzle(cmd)
                }
            }
        }
        GameId::Termwalk => {
            for (cw, n) in [(false, "↶"), (true, "↷")] {
                if ui.button(n).clicked() {
                    play.terminal(t::TerminalInput::Rotate {
                        cell: play.selected,
                        clockwise: cw,
                    })
                }
            }
        }
        GameId::Mines => {
            for (n, c) in [
                (
                    "Reveal",
                    p::PuzzleInput::Reveal {
                        cell: play.selected,
                    },
                ),
                (
                    "Flag",
                    p::PuzzleInput::Flag {
                        cell: play.selected,
                    },
                ),
                (
                    "Chord",
                    p::PuzzleInput::Chord {
                        cell: play.selected,
                    },
                ),
            ] {
                if ui.button(n).clicked() {
                    play.puzzle(c)
                }
            }
        }
        GameId::Nimesweeper => {
            for (n, c) in [
                (
                    "Reveal",
                    t::TerminalInput::Reveal {
                        cell: play.selected,
                    },
                ),
                (
                    "Flag",
                    t::TerminalInput::Flag {
                        cell: play.selected,
                    },
                ),
                (
                    "Chord",
                    t::TerminalInput::Chord {
                        cell: play.selected,
                    },
                ),
                ("Pause", t::TerminalInput::Pause),
            ] {
                if ui.button(n).clicked() {
                    play.terminal(c)
                }
            }
        }
        GameId::Ctris | GameId::Ninvaders => {
            for (n, c) in [
                ("←", t::TerminalInput::Move { dx: -1, dy: 0 }),
                ("→", t::TerminalInput::Move { dx: 1, dy: 0 }),
                ("Pause", t::TerminalInput::Pause),
            ] {
                if ui.button(n).clicked() {
                    play.terminal(c)
                }
            }
            if id == GameId::Ctris {
                for (n, c) in [
                    (
                        "Rotate",
                        t::TerminalInput::Rotate {
                            cell: 0,
                            clockwise: true,
                        },
                    ),
                    ("Down", t::TerminalInput::Move { dx: 0, dy: 1 }),
                    ("Drop", t::TerminalInput::Drop),
                ] {
                    if ui.button(n).clicked() {
                        play.terminal(c)
                    }
                }
            } else {
                if ui.button("Start").clicked() {
                    play.terminal(t::TerminalInput::Start)
                }
                if ui.button("Fire").clicked() {
                    play.terminal(t::TerminalInput::Fire)
                }
            }
        }
        GameId::Greed => {
            for (dy, names) in [
                (-1, ["↖", "↑", "↗"]),
                (0, ["←", "·", "→"]),
                (1, ["↙", "↓", "↘"]),
            ] {
                for (dx, n) in (-1..=1).zip(names) {
                    if (dx != 0 || dy != 0) && ui.button(n).clicked() {
                        play.terminal(t::TerminalInput::Move { dx, dy })
                    }
                }
            }
        }
        GameId::Guess => {
            if ui.button("Submit guess").clicked() {
                play.puzzle(p::PuzzleInput::Submit)
            }
            if ui.button("Hold selected peg").clicked() {
                play.puzzle(p::PuzzleInput::Hold {
                    slot: play.selected.min(3),
                })
            }
        }
        _ => {}
    });
}
fn palette(n: u8) -> Color32 {
    match n {
        1 => Color32::from_rgb(33, 160, 125),
        2 => Color32::from_rgb(220, 130, 60),
        3 => Color32::from_rgb(85, 145, 220),
        4 => Color32::from_rgb(175, 105, 200),
        5 => Color32::from_rgb(220, 195, 70),
        6 => Color32::from_rgb(215, 90, 115),
        _ => Color32::from_gray(55),
    }
}
fn board(
    ui: &mut Ui,
    play: &mut PlayState,
    w: usize,
    h: usize,
    labels: &[String],
    colours: &[Color32],
) -> Option<(usize, PointerButton)> {
    let size = (ui.available_width().min(880.) / w as f32).clamp(14., 38.);
    let (rect, response) =
        ui.allocate_exact_size(vec2(size * w as f32, size * h as f32), egui::Sense::click());
    play.board = Some(rect);
    for i in 0..w * h {
        let r = Rect::from_min_size(
            rect.min + vec2((i % w) as f32 * size, (i / w) as f32 * size),
            vec2(size, size),
        );
        ui.painter().rect_filled(
            r.shrink(1.),
            2.,
            colours.get(i).copied().unwrap_or(palette(0)),
        );
        if let Some(label) = labels.get(i) {
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::monospace((size * 0.50).max(9.)),
                Color32::WHITE,
            );
        }
        if i == play.selected {
            ui.painter().rect_stroke(
                r.shrink(1.),
                1.,
                Stroke::new(2., Color32::from_rgb(100, 245, 180)),
                StrokeKind::Inside,
            );
        }
    }
    let button = if response.clicked_by(PointerButton::Secondary) {
        Some(PointerButton::Secondary)
    } else if response.clicked_by(PointerButton::Middle) {
        Some(PointerButton::Middle)
    } else if response.clicked() {
        Some(PointerButton::Primary)
    } else {
        None
    };
    if let (Some(button), Some(pos)) = (button, response.interact_pointer_pos()) {
        if pos.is_finite() && rect.contains(pos) {
            let x = ((pos.x - rect.left()) / size).floor() as usize;
            let y = ((pos.y - rect.top()) / size).floor() as usize;
            if x < w && y < h {
                let i = y * w + x;
                play.selected = i;
                return Some((i, button));
            }
        }
    }
    None
}
fn pipes(
    ui: &mut Ui,
    play: &mut PlayState,
    w: usize,
    h: usize,
    tiles: &[u8],
    connected: &[bool],
    locked: Option<&[bool]>,
    source: usize,
) -> Option<(usize, PointerButton)> {
    let colours: Vec<_> = connected
        .iter()
        .map(|&c| if c { palette(1) } else { palette(0) })
        .collect();
    let labels: Vec<_> = tiles
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i == source {
                "●".into()
            } else if locked.is_some_and(|l| l[i]) {
                "◆".into()
            } else {
                String::new()
            }
        })
        .collect();
    let hit = board(ui, play, w, h, &labels, &colours);
    let r = play.board.unwrap();
    let size = r.width() / w as f32;
    for (i, &mask) in tiles.iter().enumerate() {
        let center = r.min
            + vec2(
                (i % w) as f32 * size + size / 2.,
                (i / w) as f32 * size + size / 2.,
            );
        for (bit, delta) in [
            (1, vec2(0., -size / 2.)),
            (2, vec2(size / 2., 0.)),
            (4, vec2(0., size / 2.)),
            (8, vec2(-size / 2., 0.)),
        ] {
            if mask & bit != 0 {
                ui.painter().line_segment(
                    [center, center + delta * 0.9],
                    Stroke::new(3., Color32::WHITE),
                );
            }
        }
    }
    hit
}
fn draw_scene(ui: &mut Ui, play: &mut PlayState) {
    let Some(scene) = play.session.as_ref().map(GameSession::scene) else {
        return;
    };
    match scene {
        GameScene::Puzzle(p::PuzzleScene::Net(s)) => {
            if let Some((cell, b)) = pipes(
                ui,
                play,
                s.width,
                s.height,
                &s.tiles,
                &s.connected,
                Some(&s.locked),
                s.source,
            ) {
                play.puzzle(if b == PointerButton::Middle {
                    p::PuzzleInput::Lock { cell }
                } else {
                    p::PuzzleInput::Rotate {
                        cell,
                        clockwise: b == PointerButton::Secondary,
                    }
                })
            }
        }
        GameScene::Terminal(t::TerminalScene::Termwalk(s)) => {
            if let Some((cell, b)) = pipes(
                ui,
                play,
                s.width,
                s.height,
                &s.pipes,
                &s.connected,
                None,
                s.router,
            ) {
                play.terminal(t::TerminalInput::Rotate {
                    cell,
                    clockwise: b != PointerButton::Secondary,
                })
            }
        }
        GameScene::Puzzle(p::PuzzleScene::Mines(s)) => {
            ui.label(format!(
                "{} mines · {} flags · {} deaths",
                s.mine_count, s.flags, s.deaths
            ));
            let labels = s
                .cells
                .iter()
                .map(|c| match c {
                    p::MineCell::Covered => "·".into(),
                    p::MineCell::Flagged => "⚑".into(),
                    p::MineCell::Open(0) => "".into(),
                    p::MineCell::Open(n) => n.to_string(),
                    p::MineCell::Exploded => "✹".into(),
                })
                .collect::<Vec<_>>();
            let colours = s
                .cells
                .iter()
                .map(|c| {
                    if matches!(c, p::MineCell::Open(_)) {
                        Color32::from_gray(85)
                    } else {
                        palette(0)
                    }
                })
                .collect::<Vec<_>>();
            if let Some((cell, b)) = board(ui, play, s.width, s.height, &labels, &colours) {
                play.puzzle(if b == PointerButton::Secondary {
                    p::PuzzleInput::Flag { cell }
                } else if matches!(s.cells[cell], p::MineCell::Open(_)) {
                    p::PuzzleInput::Chord { cell }
                } else {
                    p::PuzzleInput::Reveal { cell }
                })
            }
        }
        GameScene::Terminal(t::TerminalScene::Nimesweeper(s)) => {
            ui.label(format!(
                "{} mines{}",
                s.mines,
                if s.paused { " · Paused" } else { "" }
            ));
            let labels = s
                .cells
                .iter()
                .map(|c| {
                    if s.paused {
                        "".into()
                    } else {
                        match c {
                            t::MineCell::Hidden => "·".into(),
                            t::MineCell::Flagged => "⚑".into(),
                            t::MineCell::Open(0) => "".into(),
                            t::MineCell::Open(n) => n.to_string(),
                            t::MineCell::Mine => "✹".into(),
                        }
                    }
                })
                .collect::<Vec<_>>();
            if let Some((cell, b)) = board(ui, play, s.width, s.height, &labels, &[]) {
                play.terminal(if b == PointerButton::Secondary {
                    t::TerminalInput::Flag { cell }
                } else if matches!(s.cells[cell], t::MineCell::Open(_)) {
                    t::TerminalInput::Chord { cell }
                } else {
                    t::TerminalInput::Reveal { cell }
                })
            }
        }
        GameScene::Puzzle(p::PuzzleScene::Solo(s)) => {
            let labels = s
                .digits
                .iter()
                .enumerate()
                .map(|(i, &d)| {
                    if d != 0 {
                        d.to_string()
                    } else {
                        (1..=9)
                            .filter(|n| s.pencil[i] & (1 << n) != 0)
                            .map(|n| char::from(b'0' + n))
                            .collect()
                    }
                })
                .collect::<Vec<_>>();
            let colours = (0..81)
                .map(|i| {
                    if s.conflicts[i] {
                        palette(6)
                    } else if s.givens[i] {
                        Color32::from_gray(85)
                    } else {
                        palette(0)
                    }
                })
                .collect::<Vec<_>>();
            board(ui, play, 9, 9, &labels, &colours);
        }
        GameScene::Terminal(t::TerminalScene::Sudoku(s)) => {
            let labels = s
                .cells
                .iter()
                .map(|&d| if d == 0 { String::new() } else { d.to_string() })
                .collect::<Vec<_>>();
            let colours = s
                .fixed
                .iter()
                .map(|&v| {
                    if v {
                        Color32::from_gray(85)
                    } else {
                        palette(0)
                    }
                })
                .collect::<Vec<_>>();
            board(ui, play, 9, 9, &labels, &colours);
        }
        GameScene::Puzzle(p::PuzzleScene::Guess(s)) => {
            for (i, row) in s.rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", i + 1));
                    for c in row.pegs {
                        ui.colored_label(palette(c), format!("●{c}"));
                    }
                    ui.label(format!("{} exact · {} elsewhere", row.exact, row.misplaced));
                });
            }
            let labels = (0..4)
                .map(|i| format!("{}{}", s.current[i], if s.held[i] { "◆" } else { "" }))
                .collect::<Vec<_>>();
            let colours = s.current.iter().map(|&c| palette(c)).collect::<Vec<_>>();
            if let Some((slot, b)) = board(ui, play, 4, 1, &labels, &colours) {
                if b == PointerButton::Secondary {
                    play.puzzle(p::PuzzleInput::Hold { slot })
                } else {
                    play.puzzle(p::PuzzleInput::Peg {
                        slot,
                        colour: s.current[slot] % 6 + 1,
                    })
                }
            }
            if let Some(answer) = s.solution {
                ui.label(format!("Answer: {answer:?}"));
            }
        }
        GameScene::Puzzle(p::PuzzleScene::Map(s)) => {
            ui.horizontal(|ui| {
                ui.label("Paint colour:");
                for c in 1..=4 {
                    ui.selectable_value(
                        &mut play.colour,
                        c,
                        RichText::new(format!("● {c}")).color(palette(c)),
                    );
                }
            });
            let labels = s
                .regions
                .iter()
                .map(|&region| {
                    if s.givens[region] {
                        "◆".into()
                    } else if s.conflicts[region] {
                        "!".into()
                    } else if s.pencil[region] != 0 {
                        (1..=4)
                            .filter(|n| s.pencil[region] & (1 << n) != 0)
                            .map(|n| char::from(b'0' + n))
                            .collect()
                    } else {
                        String::new()
                    }
                })
                .collect::<Vec<_>>();
            let colours = s
                .regions
                .iter()
                .map(|&r| palette(s.colours[r]))
                .collect::<Vec<_>>();
            let hit = board(ui, play, s.width, s.height, &labels, &colours);
            let rect = play.board.unwrap();
            let size = rect.width() / s.width as f32;
            for i in 0..s.regions.len() {
                let x = i % s.width;
                let y = i / s.width;
                let min = rect.min + vec2(x as f32 * size, y as f32 * size);
                if x == 0 || s.regions[i - 1] != s.regions[i] {
                    ui.painter()
                        .line_segment([min, min + vec2(0., size)], Stroke::new(2., Color32::BLACK));
                }
                if y == 0 || s.regions[i - s.width] != s.regions[i] {
                    ui.painter()
                        .line_segment([min, min + vec2(size, 0.)], Stroke::new(2., Color32::BLACK));
                }
            }
            if let Some((cell, b)) = hit {
                let region = s.regions[cell];
                let colour = play.colour.max(1);
                play.puzzle(if play.pencil || b == PointerButton::Secondary {
                    p::PuzzleInput::MapPencil { region, colour }
                } else {
                    p::PuzzleInput::Colour { region, colour }
                })
            }
        }
        GameScene::Puzzle(p::PuzzleScene::Untangle(s)) => {
            let side = ui.available_width().min(520.).max(150.);
            let (rect, response) = ui.allocate_exact_size(vec2(side, side), egui::Sense::drag());
            play.board = Some(rect);
            let pos = |x: i32, y: i32| {
                rect.min
                    + vec2(
                        x as f32 / s.extent as f32 * side,
                        y as f32 / s.extent as f32 * side,
                    )
            };
            let pointer = response.interact_pointer_pos();
            if response.drag_started() {
                // A frame may contain press and motion; acquire from the press,
                // while the current pointer position continues to drive preview.
                let origin = ui.input(|i| {
                    i.events
                        .iter()
                        .rev()
                        .find_map(|event| match event {
                            Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed: true,
                                ..
                            } => Some(*pos),
                            _ => None,
                        })
                        .or_else(|| i.pointer.press_origin())
                });
                if let Some(p) = origin.filter(|p| p.is_finite() && rect.contains(*p)) {
                    play.dragging = s
                        .vertices
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| pos(v.0, v.1).distance(p) < 20.)
                        .min_by(|a, b| {
                            pos(a.1.0, a.1.1)
                                .distance(p)
                                .total_cmp(&pos(b.1.0, b.1.1).distance(p))
                        })
                        .map(|(i, _)| i);
                }
            }
            let mut vertices = s.vertices.clone();
            if let (Some(v), Some(p)) = (play.dragging, pointer) {
                if p.is_finite() {
                    vertices[v] = (
                        (((p.x - rect.left()) / side * 1000.).round() as i32).clamp(0, 1000),
                        (((p.y - rect.top()) / side * 1000.).round() as i32).clamp(0, 1000),
                    );
                }
            }
            for (i, &(a, b)) in s.edges.iter().enumerate() {
                ui.painter().line_segment(
                    [
                        pos(vertices[a].0, vertices[a].1),
                        pos(vertices[b].0, vertices[b].1),
                    ],
                    Stroke::new(
                        2.,
                        if s.crossing_edges[i] {
                            palette(6)
                        } else {
                            palette(1)
                        },
                    ),
                );
            }
            for (i, &(x, y)) in vertices.iter().enumerate() {
                ui.painter().circle_filled(
                    pos(x, y),
                    9.,
                    if Some(i) == play.dragging || i == play.selected {
                        palette(5)
                    } else {
                        palette(3)
                    },
                );
            }
            if response.drag_stopped() {
                if let Some(v) = play.dragging.take() {
                    let (x, y) = vertices[v];
                    play.puzzle(p::PuzzleInput::Move { vertex: v, x, y });
                }
            }
        }
        GameScene::Terminal(t::TerminalScene::Greed(s)) => {
            ui.label(format!("{} squares erased", s.erased));
            let labels = s
                .cells
                .iter()
                .enumerate()
                .map(|(i, &n)| {
                    if i == s.player {
                        "@".into()
                    } else if n == 0 {
                        String::new()
                    } else {
                        n.to_string()
                    }
                })
                .collect::<Vec<_>>();
            if let Some((cell, _)) = board(ui, play, s.width, s.height, &labels, &[]) {
                let dx = (cell % s.width) as i32 - (s.player % s.width) as i32;
                let dy = (cell / s.width) as i32 - (s.player / s.width) as i32;
                if dx == 0 || dy == 0 || dx.abs() == dy.abs() {
                    play.terminal(t::TerminalInput::Move {
                        dx: dx.signum() as i8,
                        dy: dy.signum() as i8,
                    })
                }
            }
        }
        GameScene::Terminal(t::TerminalScene::Ctris(s)) => {
            ui.label(format!(
                "Score {} · Lines {} · Next {}{}",
                s.score,
                s.lines,
                s.next,
                if s.paused { " · Paused" } else { "" }
            ));
            let mut colours = s.settled.iter().map(|&n| palette(n)).collect::<Vec<_>>();
            for (x, y) in s.active.cells() {
                if x >= 0 && y >= 0 && (x as usize) < s.width && (y as usize) < s.height {
                    colours[y as usize * s.width + x as usize] = palette(s.active.kind + 1);
                }
            }
            board(ui, play, s.width, s.height, &[], &colours);
        }
        GameScene::Terminal(t::TerminalScene::Ninvaders(s)) => {
            ui.label(format!(
                "Lives {} · Score {}{}{}",
                s.lives,
                s.score,
                if s.started { "" } else { " · Press Start" },
                if s.paused { " · Paused" } else { "" }
            ));
            let w = s.width as usize;
            let h = s.height as usize;
            let mut labels = vec![String::new(); w * h];
            let mut mark = |x: i32, y: i32, ch: &str| {
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    labels[y as usize * w + x as usize] = ch.into()
                }
            };
            for e in s.enemies {
                mark(e.x, e.y, "W")
            }
            for (x, y) in s.player_shots {
                mark(x, y, "↑")
            }
            for (x, y) in s.enemy_shots {
                mark(x, y, "↓")
            }
            mark(s.player_x, s.player_y, "▲");
            board(ui, play, w, h, &labels, &[]);
        }
        GameScene::Terminal(t::TerminalScene::Soliterm(s)) => {
            ui.label(format!(
                "{:?} · Stock {} · Passes left {}",
                s.variant, s.stock_count, s.passes_left
            ));
            if ui.button("Draw from stock").clicked() {
                play.terminal(t::TerminalInput::Draw)
            }
            ui.horizontal(|ui| {
                for i in 0..4 {
                    if ui
                        .button(format!(
                            "Reserve {}: {}",
                            i + 1,
                            s.reserve[i].map_or("—".into(), card)
                        ))
                        .clicked()
                    {
                        pile_select(play, t::CardPile::Reserve(i))
                    }
                }
            });
            ui.horizontal(|ui| {
                for i in 0..4 {
                    if ui
                        .button(format!("Foundation {}: {}", i + 1, s.foundations[i]))
                        .clicked()
                    {
                        pile_select(play, t::CardPile::Foundation(i))
                    }
                }
            });
            if ui
                .button(format!(
                    "Waste: {}",
                    s.waste.last().copied().map_or("—".into(), card)
                ))
                .clicked()
            {
                pile_select(play, t::CardPile::Waste)
            }
            ui.add(egui::Slider::new(&mut play.count, 1..=13).text("Cards to move"));
            ui.label(format!("Selected source: {:?}. Keyboard pile {}: reserves 1–4, foundations 5–8, waste 9, columns 10 onward. Select a destination pile; click source again to clear.",play.source,play.selected+1));
            ui.horizontal_top(|ui| {
                for (i, column) in s.tableau.iter().enumerate() {
                    ui.vertical(|ui| {
                        if ui.button(format!("Column {}", i + 1)).clicked() {
                            pile_select(play, t::CardPile::Tableau(i))
                        }
                        for &c in column {
                            ui.label(card(c));
                        }
                    });
                }
            });
        }
    }
}
fn card(c: t::Card) -> String {
    if !c.face_up {
        return "▧".into();
    }
    let rank = match c.rank {
        1 => "A".into(),
        11 => "J".into(),
        12 => "Q".into(),
        13 => "K".into(),
        n => n.to_string(),
    };
    format!(
        "{rank}{}",
        ["♣", "♦", "♥", "♠"].get(c.suit as usize).unwrap_or(&"?")
    )
}
fn pile_select(play: &mut PlayState, to: t::CardPile) {
    if let Some(from) = play.source.take() {
        if from != to {
            play.terminal(t::TerminalInput::Transfer {
                from,
                to,
                count: play.count.max(1),
            })
        }
    } else {
        play.source = Some(to)
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ui::terminal_review_tests::{captured, owned_workspace, raw_terminal};
    fn key(k: Key, pressed: bool) -> Event {
        Event::Key {
            key: k,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        }
    }
    fn frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>) {
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 1000.))),
                events,
                ..Default::default()
            },
            |ui| w.render(ui),
        );
        out.textures_delta.clear();
    }
    #[test]
    fn all_thirteen_real_boards_render_and_game_keys_cannot_reach_owned_broadcast_sessions() {
        let (ctx, _dir, mut w) = owned_workspace();
        let a = raw_terminal(&mut w, "game-isolation-a");
        let b = raw_terminal(&mut w, "game-isolation-b");
        w.broadcast = true;
        let identities = w
            .tabs
            .iter()
            .map(|t| t.process.as_ref().unwrap() as *const Process)
            .collect::<Vec<_>>();
        w.tabs[0].send(b"background-a");
        w.tabs[1].send(b"background-b");
        for spec in GameRegistry::catalog() {
            w.games.play.start(spec.id, 7);
            if spec.id == GameId::Ninvaders {
                frame(
                    &ctx,
                    &mut w,
                    vec![key(Key::Space, true), key(Key::Space, false)],
                );
            }
            let before = w.games.play.session.as_ref().unwrap().scene();
            frame(
                &ctx,
                &mut w,
                vec![
                    Event::Text("never-send".into()),
                    Event::Paste("never-paste".into()),
                    key(Key::ArrowRight, true),
                    key(Key::ArrowRight, false),
                ],
            );
            assert!(ctx.input(|i| i.events.is_empty()));
            assert_eq!(w.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![a, b]);
            assert!(w.games.play.session.is_some());
            if spec.id == GameId::Ctris || spec.id == GameId::Ninvaders {
                assert_ne!(
                    w.games.play.session.as_ref().unwrap().scene(),
                    before,
                    "real move through game keyboard"
                );
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while (captured(&w.tabs[0]).len() < 12 || captured(&w.tabs[1]).len() < 12)
            && Instant::now() < deadline
        {
            frame(&ctx, &mut w, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(captured(&w.tabs[0]), b"background-a");
        assert_eq!(captured(&w.tabs[1]), b"background-b");
        assert_eq!(
            w.tabs
                .iter()
                .map(|t| t.process.as_ref().unwrap() as *const Process)
                .collect::<Vec<_>>(),
            identities
        );
    }
    #[test]
    fn keyboard_pointer_moves_reset_and_undo_reach_actual_core() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        frame(&ctx, &mut w, vec![]);
        let initial = w.games.play.session.as_ref().unwrap().scene();
        frame(&ctx, &mut w, vec![key(Key::S, true), key(Key::S, false)]);
        assert_ne!(w.games.play.session.as_ref().unwrap().scene(), initial);
        assert!(w.games.play.session.as_mut().unwrap().undo());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), initial);
        let r = w.games.play.board.unwrap();
        let pos = r.min + vec2(10., 10.);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut w,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Middle,
                        pressed,
                        modifiers: Modifiers::default(),
                    },
                ],
            );
        }
        assert_ne!(
            w.games.play.session.as_ref().unwrap().scene(),
            initial,
            "middle pointer really toggles core lock"
        );
        w.games.play.session.as_mut().unwrap().reset();
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), initial);
    }
    #[test]
    fn close_consumes_escape_repeats_release_and_recovers_canceled_pointer() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.play.start(GameId::Mines, 3);
        frame(&ctx, &mut w, vec![key(Key::Escape, true)]);
        assert!(!w.games.play.is_open());
        assert_eq!(w.games.play.close_owner, Some(ViewportId::ROOT));
        frame(
            &ctx,
            &mut w,
            vec![key(Key::Escape, true), Event::Text("hidden".into())],
        );
        assert!(ctx.input(|i| i.events.is_empty()));
        frame(
            &ctx,
            &mut w,
            vec![key(Key::Escape, false), Event::Text("release".into())],
        );
        assert!(ctx.input(|i| i.events.is_empty()));
        assert!(w.games.play.close_owner.is_none());
        w.games.play.close_owner = Some(ViewportId::ROOT);
        frame(&ctx, &mut w, vec![Event::PointerGone]);
        assert!(w.games.play.close_owner.is_none());
        assert!(!ctx.input(|i| i.pointer.any_down()));
    }
    #[test]
    fn library_retains_session_and_new_variants_reset_to_selected_rules() {
        let mut play = PlayState::default();
        play.variant = 1;
        play.passes = 2;
        play.start(GameId::Soliterm, 1);
        let before = play.session.as_ref().unwrap().scene();
        play.open();
        assert_eq!(play.session.as_ref().unwrap().scene(), before);
        play.launcher = false;
        let GameScene::Terminal(t::TerminalScene::Soliterm(s)) = before else {
            panic!()
        };
        assert_eq!(s.variant, t::SolitaireVariant::Klondike);
        assert_eq!(s.passes_left, 2);
        play.walk_size = 9;
        play.start(GameId::Termwalk, 2);
        let GameScene::Terminal(t::TerminalScene::Termwalk(s)) =
            play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert_eq!((s.width, s.height), (9, 9));
        play.portrait = true;
        play.start(GameId::Map, 2);
        let GameScene::Puzzle(p::PuzzleScene::Map(s)) = play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert_eq!((s.width, s.height), (16, 18));
    }
    #[test]
    fn untangle_keyboard_move_uses_selected_vertex_and_bounded_integer_coordinates() {
        let mut play = PlayState::default();
        play.start(GameId::Untangle, 1);
        let before = play.session.as_ref().unwrap().scene();
        play.key(Key::ArrowRight, Modifiers::default());
        assert_ne!(play.session.as_ref().unwrap().scene(), before);
        assert!(play.session.as_mut().unwrap().undo());
        assert_eq!(play.session.as_ref().unwrap().scene(), before);
    }
    #[test]
    fn native_close_suspends_game_without_destroying_it_and_shows_owned_session_confirmation() {
        let (ctx, _dir, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "native-game-close");
        w.games.play.start(GameId::Netwalk, 2);
        let saved = w.games.play.session.as_ref().unwrap().scene();
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 1000.))),
            ..Default::default()
        };
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut out = ctx.run_ui(raw, |ui| {
            w.intercept_session_close(ui.ctx());
            w.render(ui)
        });
        assert!(
            out.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        out.textures_delta.clear();
        assert!(w.pending_session_close.is_some());
        assert!(!w.games.play.is_open());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), saved);
        assert_eq!(w.tabs[0].id, id);
        w.resolve_session_close(false, &ctx);
        w.games.play.open();
        assert!(w.games.play.is_open());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), saved);
    }
    #[test]
    fn detached_native_close_is_canceled_while_game_retains_owned_terminal() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (ctx, _dir, mut w) = owned_workspace();
        let id = raw_terminal(&mut w, "detached-game-close");
        let process = w.tabs[0].process.as_ref().unwrap() as *const Process;
        w.detach_terminal(id);
        let window = w.windows.window(id);
        let child = crate::ui::windows::viewport_id(window);
        w.games.play.start(GameId::Mines, 4);
        let saved = w.games.play.session.as_ref().unwrap().scene();
        let once = Arc::new(AtomicBool::new(true));
        let seen = Arc::new(AtomicBool::new(false));
        let visible = seen.clone();
        ctx.set_embed_viewports(false);
        Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let mut raw = RawInput {
                viewport_id: viewport.ids.this,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960., 640.))),
                ..Default::default()
            };
            raw.viewports.insert(
                viewport.ids.this,
                egui::ViewportInfo {
                    parent: Some(viewport.ids.parent),
                    events: if once.swap(false, Ordering::SeqCst) {
                        vec![egui::ViewportEvent::Close]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
            );
            let mut out = ctx.run_ui(raw, |ui| (viewport.viewport_ui_cb)(ui));
            if out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text.contains("Reattach these terminals"))){visible.store(true,Ordering::SeqCst);}
            out.textures_delta.clear();
        });
        let mut raw = RawInput::default();
        raw.viewports.insert(child, egui::ViewportInfo::default());
        let mut out = ctx.run_ui(raw, |ui| w.detached_windows(ui.ctx()));
        assert!(
            out.viewport_output[&child]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        out.textures_delta.clear();
        assert!(seen.load(Ordering::SeqCst));
        assert!(w.window_close_prompt.contains(&window));
        assert!(!w.games.play.is_open());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), saved);
        assert_eq!(
            w.tabs[0].process.as_ref().unwrap() as *const Process,
            process
        );
    }
    #[test]
    fn canceled_pointer_keeps_other_held_close_keys_owned_until_their_release() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.play.start(GameId::Mines, 4);
        frame(&ctx, &mut w, vec![key(Key::Escape, true)]);
        frame(&ctx, &mut w, vec![Event::PointerGone]);
        assert_eq!(w.games.play.close_owner, Some(ViewportId::ROOT));
        frame(&ctx, &mut w, vec![key(Key::Escape, false)]);
        assert!(w.games.play.close_owner.is_none());
    }
    #[test]
    fn invader_render_player_marker_uses_core_player_row() {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.play.start(GameId::Ninvaders, 1);
        let GameScene::Terminal(t::TerminalScene::Ninvaders(scene)) =
            w.games.play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 1000.))),
                ..Default::default()
            },
            |ui| {
                w.games_frame(ui);
            },
        );
        let board = w.games.play.board.unwrap();
        let cell = board.width() / scene.width as f32;
        let expected = board.min
            + vec2(
                (scene.player_x as f32 + 0.5) * cell,
                (scene.player_y as f32 + 0.5) * cell,
            );
        let correct = out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text=="▲"&&(t.pos+t.galley.rect.center().to_vec2()).distance(expected)<1.));
        out.textures_delta.clear();
        assert!(
            correct,
            "rendered marker must use the actual core collision row"
        );
    }

    #[test]
    fn cards_keyboard_source_and_destination_apply_a_real_transfer() {
        let mut play = PlayState::default();
        play.start(GameId::Soliterm, 1);
        let before = play.session.as_ref().unwrap().scene();
        play.selected = 9;
        play.key(Key::Enter, Modifiers::default());
        play.selected = 0;
        play.key(Key::Space, Modifiers::default());
        assert_ne!(play.session.as_ref().unwrap().scene(), before);
        assert!(play.session.as_mut().unwrap().undo());
        assert_eq!(play.session.as_ref().unwrap().scene(), before);
    }
    #[test]
    fn closing_main_game_consumes_a_held_game_key_and_release_in_another_viewport() {
        let (ctx, _dir, mut w) = owned_workspace();
        let child = ViewportId::from_hash_of("game-held-child");
        ctx.set_embed_viewports(false);
        w.games.play.start(GameId::Mines, 1);
        let mut raw = RawInput {
            viewport_id: child,
            events: vec![key(Key::ArrowLeft, true)],
            ..Default::default()
        };
        raw.viewports.insert(
            child,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                focused: Some(true),
                ..Default::default()
            },
        );
        let mut out = ctx.run_ui(raw, |ui| {
            assert!(w.games_frame(ui));
        });
        out.textures_delta.clear();
        let mut raw = RawInput {
            events: vec![key(Key::Escape, true), key(Key::Escape, false)],
            ..Default::default()
        };
        raw.viewports.insert(
            child,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                focused: Some(true),
                ..Default::default()
            },
        );
        let mut out = ctx.run_ui(raw, |ui| {
            ui.ctx()
                .show_viewport_deferred(child, ViewportBuilder::default(), |_, _| {});
            assert!(w.games_frame(ui));
        });
        out.textures_delta.clear();
        assert_eq!(
            w.games.play.close_owner,
            Some(child),
            "held game input in child must not be released by closing root"
        );
        let mut raw = RawInput {
            viewport_id: child,
            events: vec![
                key(Key::ArrowLeft, false),
                Event::Text("swallowed-release".into()),
            ],
            ..Default::default()
        };
        raw.viewports.insert(
            child,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                focused: Some(true),
                ..Default::default()
            },
        );
        let mut out = ctx.run_ui(raw, |ui| {
            assert!(w.games_frame(ui));
        });
        out.textures_delta.clear();
        assert!(w.games.play.close_owner.is_none());
        assert!(ctx.input_for(child, |i| i.events.is_empty()));
    }
}

#[cfg(all(test, unix))]
mod untangle_fix1_regression {
    use super::*;
    use crate::ui::terminal_review_tests::owned_workspace;
    fn frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>) {
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1320., 1000.))),
                events,
                ..Default::default()
            },
            |ui| {
                w.games_frame(ui);
            },
        );
        out.textures_delta.clear();
    }
    fn drag(fast: bool) {
        let (ctx, _dir, mut w) = owned_workspace();
        w.games.play.start(GameId::Untangle, 1);
        frame(&ctx, &mut w, vec![]);
        frame(&ctx, &mut w, vec![]);
        let before = w.games.play.session.as_ref().unwrap().scene();
        let GameScene::Puzzle(p::PuzzleScene::Untangle(s)) = &before else {
            panic!()
        };
        let r = w.games.play.board.unwrap();
        let (x, y) = s.vertices[0];
        let origin = r.min
            + vec2(
                x as f32 / s.extent as f32 * r.width(),
                y as f32 / s.extent as f32 * r.height(),
            );
        let dest = origin + vec2(65., 20.);
        assert!(r.contains(origin) && r.contains(dest));
        let press = Event::PointerButton {
            pos: origin,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        };
        frame(&ctx, &mut w, vec![Event::PointerMoved(origin)]);
        if fast {
            frame(&ctx, &mut w, vec![press, Event::PointerMoved(dest)]);
        } else {
            frame(&ctx, &mut w, vec![press]);
            frame(&ctx, &mut w, vec![Event::PointerMoved(dest)]);
        }
        frame(
            &ctx,
            &mut w,
            vec![Event::PointerButton {
                pos: dest,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        let after = w.games.play.session.as_ref().unwrap().scene();
        assert_ne!(
            after, before,
            "Untangle actual drag must move vertex (fast={fast})"
        );
        let mut expected = GameRegistry::start(GameId::Untangle, 1);
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Move {
                vertex: 0,
                x: (((dest.x - r.left()) / r.width() * 1000.).round() as i32).clamp(0, 1000),
                y: (((dest.y - r.top()) / r.height() * 1000.).round() as i32).clamp(0, 1000),
            }))
            .unwrap();
        assert_eq!(
            after,
            expected.scene(),
            "drag must move the pressed vertex to the bounded current position"
        );
        assert!(w.games.play.session.as_mut().unwrap().undo());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        assert!(
            !w.games.play.session.as_ref().unwrap().can_undo(),
            "one drag is one undo action"
        );
    }
    #[test]
    fn untangle_separate_press_drag() {
        drag(false)
    }
    #[test]
    fn untangle_batched_press_motion_drag() {
        drag(true)
    }
}
