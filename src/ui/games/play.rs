use super::save::{Action as FileAction, Files};
use super::*;
use harbor_workspace::games::{
    GameId, GameInput, GameOutcome, GameRegistry, GameScene, GameSession, puzzles as p,
    terminal as t,
};
pub(in crate::ui) struct PlayState {
    files: Files,
    exit_ready: bool,
    next_seed: u64,
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
    draft: Option<p::PuzzleConfig>,
    solve_confirm: bool,
    walk_size: usize,
    dragging: Option<usize>,
    board_focus: Option<Id>,
    pub(in crate::ui) board: Option<Rect>,
}
impl Default for PlayState {
    fn default() -> Self {
        Self {
            files: Files::default(),
            exit_ready: false,
            next_seed: 1,
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
            draft: None,
            solve_confirm: false,
            walk_size: 5,
            dragging: None,
            board_focus: None,
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
        self.next_seed = seed.wrapping_add(1);
        self.files.replaced();
        self.draft = self.session.as_ref().and_then(GameSession::configuration);
        self.solve_confirm = false;
        self.launcher = false;
        self.suspended = false;
        self.selected = 0;
        self.source = None;
        self.count = 1;
        self.message.clear();
        self.tick = None;
        self.dragging = None;
        self.board = None;
        self.board_focus = None;
    }
    fn start_config(&mut self, seed: u64, config: p::PuzzleConfig) -> bool {
        match GameSession::new_puzzle(seed, config) {
            Ok(session) => {
                self.next_seed = seed.wrapping_add(1);
                self.files.replaced();
                self.session = Some(session);
                self.draft = Some(config);
                self.launcher = false;
                self.suspended = false;
                self.selected = 0;
                self.source = None;
                self.count = 1;
                self.message.clear();
                self.tick = None;
                self.dragging = None;
                self.board = None;
                self.board_focus = None;
                self.solve_confirm = false;
                true
            }
            Err(error) => {
                self.message = player_error(error);
                false
            }
        }
    }
    fn apply(&mut self, i: GameInput) {
        if let Some(s) = &mut self.session {
            match s.input(i) {
                Ok(changed) => {
                    if changed {
                        self.files.changed();
                    }
                    self.message.clear();
                }
                Err(e) => self.message = player_error(e),
            }
        }
    }
    fn request_file_action(&mut self, action: FileAction, ctx: &Context) {
        if self.files.confirming() {
            return;
        }
        if self.files.dirty() {
            self.suspended = false;
            self.files.pending = Some(action);
        } else {
            self.perform_file_action(action, ctx);
        }
    }
    fn perform_file_action(&mut self, action: FileAction, ctx: &Context) {
        match action {
            FileAction::Start(id, seed) => self.start(id, seed),
            FileAction::New => {
                if let Some(s) = self.session.as_ref() {
                    let (id, seed) = (s.id(), self.next_seed);
                    // Consume an attempt only after the New action is confirmed.
                    // A bounded generation failure keeps the active game intact,
                    // while the next explicit attempt can try a fresh seed.
                    self.next_seed = seed.wrapping_add(1);
                    if let Some(c) = self.draft {
                        self.start_config(seed, c);
                    } else {
                        self.start(id, seed);
                    }
                }
            }
            FileAction::Reset => {
                if let Some(s) = self.session.as_mut() {
                    s.reset();
                    self.next_seed = s.seed().wrapping_add(1);
                    self.files.replaced();
                    self.tick = None;
                    self.dragging = None;
                    self.source = None;
                    self.selected = 0;
                    self.solve_confirm = false;
                    self.message.clear();
                }
            }
            FileAction::Close => {
                self.files.replaced();
                self.session = None;
                self.next_seed = 1;
                self.launcher = false;
                self.tick = None;
                self.dragging = None;
                self.board = None;
                self.board_focus = None;
                self.close_owner = held_game_viewport(ctx);
            }
            FileAction::Exit => {
                self.perform_file_action(FileAction::Close, ctx);
                self.exit_ready = true;
            }
            FileAction::Load => self.files.start(None, ctx.clone()),
            FileAction::Adopt(session) => self.adopt_file(session, ctx),
        }
    }
    fn adopt_file(&mut self, session: GameSession, ctx: &Context) {
        use harbor_workspace::games::persistence::InitialSettings;
        self.variant = 0;
        self.passes = 3;
        self.preset = 1;
        self.walk_size = 5;
        self.portrait = false;
        match session.initial_settings() {
            InitialSettings::Solitaire { variant, passes } => {
                self.variant = usize::from(variant == t::SolitaireVariant::Klondike);
                self.passes = passes;
            }
            InitialSettings::Mines(preset) => {
                self.preset = match preset {
                    t::MinePreset::Newbie => 0,
                    t::MinePreset::Easy => 1,
                    t::MinePreset::Medium => 2,
                    t::MinePreset::Hard => 3,
                };
            }
            InitialSettings::Termwalk { size } => self.walk_size = size,
            _ => {}
        }
        self.draft = session.configuration();
        if let Some(p::PuzzleConfig::Map(c)) = self.draft {
            self.portrait = c.preset == p::MapPreset::Portrait;
        }
        self.next_seed = session.seed().wrapping_add(1);
        self.files.replaced();
        self.session = Some(session);
        self.launcher = false;
        self.suspended = false;
        self.selected = 0;
        self.colour = 1;
        self.pencil = false;
        self.help = false;
        self.source = None;
        self.count = 1;
        self.tick = None;
        self.dragging = None;
        self.board = None;
        self.solve_confirm = false;
        self.board_focus = None;
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        // Inputs queued for the previous board must not act on the candidate.
        ctx.input_mut(|i| {
            i.events.clear();
            i.pointer = Default::default();
            i.keys_down.clear();
            i.modifiers = Default::default();
        });
        self.message = "Game loaded (TermCraft format).".into();
    }
    fn file_confirmation(&mut self, ctx: &Context) {
        if let Some(prompt) = &self.files.prompt {
            let mut decision = None;
            let size = fluent::dialog_size(ctx, 460., 112.);
            let modal = fluent::modal(
                ctx,
                Id::new("game_file_replace"),
                "Replace saved game?",
                size,
                |ui| {
                    ui.label(format!("Replace {} with this game?", prompt.path.display()));
                    let [replace, safe] =
                        fluent::dialog_actions(ui, ["Replace file", "Keep existing file"], None);
                    if replace.clicked() {
                        decision = Some(true);
                    }
                    fluent::focus_on_entry(ui, &safe, ui.id());
                    if safe.clicked() {
                        decision = Some(false);
                    }
                },
            );
            if modal.should_close() {
                decision = Some(false);
            }
            if let Some(answer) = decision {
                if let Some(prompt) = self.files.prompt.take() {
                    let _ = prompt.answer.try_send(answer);
                }
            }
            return;
        }
        if self.files.pending.is_none() {
            return;
        }
        let mut discard = false;
        let mut cancel = false;
        let size = fluent::dialog_size(ctx, 460., 112.);
        let modal = fluent::modal(
            ctx,
            Id::new("game_file_discard"),
            "Unsaved game changes",
            size,
            |ui| {
                ui.label("Continuing discards changes to this game. Save first to keep them.");
                let [lose, safe] = fluent::dialog_actions(
                    ui,
                    ["Discard changes and continue", "Keep playing"],
                    None,
                );
                discard = lose.clicked();
                fluent::focus_on_entry(ui, &safe, ui.id());
                cancel = safe.clicked();
            },
        );
        if cancel || modal.should_close() {
            self.files.pending = None;
        }
        if discard {
            if let Some(action) = self.files.pending.take() {
                self.perform_file_action(action, ctx);
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
            Some(GameScene::Puzzle(p::PuzzleScene::Solo(s))) => (s.width, s.height),
            Some(GameScene::Puzzle(p::PuzzleScene::Guess(s))) => (s.current.len(), 1),
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
        if m.ctrl && (key == Key::Y || (m.shift && key == Key::Z)) {
            if let Some(s) = &mut self.session {
                if s.redo() {
                    self.files.changed();
                }
            }
            return;
        }
        if id == GameId::Guess && key == Key::A {
            self.number(10);
            return;
        }
        if id == GameId::Solo {
            let n = match key {
                Key::A => Some(10),
                Key::B => Some(11),
                Key::C => Some(12),
                Key::D => Some(13),
                Key::E => Some(14),
                Key::F => Some(15),
                Key::G => Some(16),
                _ => None,
            };
            if let Some(n) = n {
                self.number(n);
                return;
            }
        }
        if m.ctrl && key == Key::Z {
            if let Some(s) = &mut self.session {
                if s.undo() {
                    self.files.changed();
                }
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
                slot: self.selected.min(self.dimensions().0 - 1),
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
                slot: self.selected.min(self.dimensions().0 - 1),
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
fn player_error(error: &str) -> String {
    if error.contains("budget") || error.contains("exhaust") {
        "Could not create a board with these settings. Try another seed or change the settings. Your current game is kept.".into()
    } else {
        error.into()
    }
}
fn symmetry_name(sym: p::SoloSymmetry) -> &'static str {
    match sym {
        p::SoloSymmetry::None => "No symmetry",
        p::SoloSymmetry::Rotate2 => "Half-turn symmetry",
        p::SoloSymmetry::Rotate4 => "Quarter-turn symmetry",
        p::SoloSymmetry::Reflect2 => "Mirror symmetry",
        p::SoloSymmetry::ReflectDiagonal => "Diagonal mirror",
        p::SoloSymmetry::Reflect4 => "Horizontal and vertical mirrors",
        p::SoloSymmetry::Reflect4Diagonal => "Both diagonal mirrors",
        p::SoloSymmetry::Reflect8 => "All mirrors and turns",
    }
}
fn solo_options(ui: &mut Ui, c: &mut p::SoloConfig) {
    egui::ComboBox::from_id_salt("solo_boxes")
        .selected_text(format!("{}×{} boxes", c.block_width, c.block_height))
        .show_ui(ui, |ui| {
            for (w, h) in [(2, 2), (2, 3), (3, 2), (3, 3), (3, 4), (4, 3), (4, 4)] {
                let selected = (c.block_width, c.block_height) == (w, h);
                if ui
                    .selectable_label(selected, format!("{w}×{h} boxes"))
                    .clicked()
                {
                    c.block_width = w;
                    c.block_height = h;
                }
            }
        });
    ui.checkbox(&mut c.diagonal, "X diagonals (9×9 only)");
    ui.selectable_value(&mut c.difficulty, p::SoloDifficulty::Trivial, "Trivial");
    ui.selectable_value(&mut c.difficulty, p::SoloDifficulty::Basic, "Basic");
    for (value, label) in [
        (p::SoloDifficulty::Intermediate, "Intermediate"),
        (p::SoloDifficulty::Advanced, "Advanced"),
        (p::SoloDifficulty::Extreme, "Extreme"),
        (p::SoloDifficulty::Unreasonable, "Unreasonable"),
    ] {
        ui.selectable_value(&mut c.difficulty, value, label);
    }
    egui::ComboBox::from_id_salt("solo_symmetry")
        .selected_text(symmetry_name(c.symmetry))
        .show_ui(ui, |ui| {
            for sym in [
                p::SoloSymmetry::None,
                p::SoloSymmetry::Rotate2,
                p::SoloSymmetry::Rotate4,
                p::SoloSymmetry::Reflect2,
                p::SoloSymmetry::ReflectDiagonal,
                p::SoloSymmetry::Reflect4,
                p::SoloSymmetry::Reflect4Diagonal,
                p::SoloSymmetry::Reflect8,
            ] {
                ui.selectable_value(&mut c.symmetry, sym, symmetry_name(sym));
            }
        });
}
fn solo_variant_flags(ui: &mut Ui, jigsaw: &mut bool, killer: &mut bool) {
    ui.checkbox(jigsaw, "Jigsaw regions (9×9 only)");
    ui.checkbox(killer, "Killer cages (9×9 only)");
    ui.label("Advanced grades and region variants require 9×9. Killer uses 3×3 boxes, Trivial, no symmetry, with X and Jigsaw off.");
}
fn solo_variant_name(c: p::SoloVariantConfig) -> &'static str {
    if c.jigsaw {
        "Jigsaw"
    } else if c.killer {
        "Killer"
    } else {
        "Regular"
    }
}
fn puzzle_options(ui: &mut Ui, play: &mut PlayState) {
    let Some(current) = play.session.as_ref().and_then(GameSession::configuration) else {
        return;
    };
    ui.collapsing("Puzzle options — apply with New game", |ui| {
        let mut draft = play.draft.unwrap_or(current);
        egui::ComboBox::from_id_salt("puzzle_preset")
            .height(420.)
            .selected_text("Choose preset…")
            .show_ui(ui, |ui| {
                for preset in current.kind().presets() {
                    if let Some(config) = preset.configuration {
                        if ui.selectable_label(draft == config, &preset.name).clicked() {
                            draft = config;
                        }
                    } else {
                        let label = format!("{} — unavailable", preset.name);
                        ui.add_enabled(false, egui::Button::new(label));
                    }
                }
            });
        ui.label("Custom settings:");
        let mut topology_update = None;
        ui.horizontal_wrapped(|ui| match &mut draft {
            p::PuzzleConfig::Net(c) => {
                ui.add(egui::Slider::new(&mut c.width, 3..=16).text("width"));
                ui.add(egui::Slider::new(&mut c.height, 3..=16).text("height"));
                ui.checkbox(&mut c.wrapping, "Wrap opposite edges");
                ui.add(egui::Slider::new(&mut c.barrier_percent, 0..=100).text("barriers %"));
                ui.checkbox(&mut c.unique, "Require unique solution");
            }
            p::PuzzleConfig::Mines(c) => {
                ui.add(egui::Slider::new(&mut c.width, 3..=30).text("width"));
                ui.add(egui::Slider::new(&mut c.height, 3..=16).text("height"));
                ui.add(egui::Slider::new(&mut c.mines, 1..=471).text("mines"));
                ui.checkbox(&mut c.solvable, "Ensure no-guess solution");
            }
            p::PuzzleConfig::Guess(c) => {
                ui.add(egui::Slider::new(&mut c.colours, 2..=10).text("colours"));
                ui.add(egui::Slider::new(&mut c.pegs, 2..=8).text("pegs"));
                ui.add(egui::Slider::new(&mut c.max_guesses, 1..=32).text("guesses"));
                ui.checkbox(&mut c.allow_blank, "Allow blanks");
                ui.checkbox(&mut c.allow_multiple, "Allow repeated colours");
            }
            p::PuzzleConfig::Untangle { points } => {
                ui.add(egui::Slider::new(points, 4..=25).text("points"));
            }
            p::PuzzleConfig::Solo(c) => {
                solo_options(ui, c);
                let (mut jigsaw, mut killer) = (false, false);
                solo_variant_flags(ui, &mut jigsaw, &mut killer);
                if jigsaw || killer {
                    topology_update = Some(p::PuzzleConfig::SoloVariant(p::SoloVariantConfig { base: *c, jigsaw, killer }));
                }
            }
            p::PuzzleConfig::SoloVariant(c) => {
                solo_options(ui, &mut c.base);
                solo_variant_flags(ui, &mut c.jigsaw, &mut c.killer);
            }
            p::PuzzleConfig::Map(c) => {
                ui.selectable_value(&mut c.preset, p::MapPreset::Default, "Landscape");
                ui.selectable_value(&mut c.preset, p::MapPreset::Portrait, "Portrait");
                ui.selectable_value(&mut c.regions, 30, "30 regions");
                ui.selectable_value(&mut c.regions, 75, "75 regions");
                ui.selectable_value(&mut c.difficulty, p::MapDifficulty::Easy, "Easy");
                ui.selectable_value(&mut c.difficulty, p::MapDifficulty::Normal, "Normal");
                ui.selectable_value(&mut c.difficulty, p::MapDifficulty::Hard, "Hard");
                ui.selectable_value(&mut c.difficulty, p::MapDifficulty::Unreasonable, "Unreasonable");
            }
        });
        if let Some(updated) = topology_update { draft = updated; }
        if let Err(error) = draft.validate() {
            ui.colored_label(palette(2), error);
        }
        ui.label("Your current board is kept if a new board cannot be made. Try another seed or simpler settings if generation cannot finish.");
        play.draft = Some(draft);
    });
    ui.label(match current {
        p::PuzzleConfig::Net(c) => format!(
            "Current: {}×{} · {} · barriers {}%",
            c.width,
            c.height,
            if c.wrapping {
                "wrapping"
            } else {
                "closed edges"
            },
            c.barrier_percent
        ),
        p::PuzzleConfig::Mines(c) => format!(
            "Current: {}×{} · {} mines · {}",
            c.width,
            c.height,
            c.mines,
            if c.solvable {
                "no-guess required"
            } else {
                "guessing allowed"
            }
        ),
        p::PuzzleConfig::SoloVariant(c) => format!(
            "Current: 9×9 · {} · {:?}{}",
            solo_variant_name(c),
            c.base.difficulty,
            if c.base.diagonal {
                " · X diagonals"
            } else {
                ""
            }
        ),
        p::PuzzleConfig::Solo(c) => format!(
            "Current: {}×{} · {}×{} boxes{} · {:?}",
            c.block_width * c.block_height,
            c.block_width * c.block_height,
            c.block_width,
            c.block_height,
            if c.diagonal { " · X diagonals" } else { "" },
            c.difficulty
        ),
        p::PuzzleConfig::Guess(c) => format!(
            "Current: {} pegs · {} colours · {} guesses · {} · {}",
            c.pegs,
            c.colours,
            c.max_guesses,
            if c.allow_blank {
                "blanks allowed"
            } else {
                "fill all pegs"
            },
            if c.allow_multiple {
                "repeats allowed"
            } else {
                "distinct colours"
            }
        ),
        p::PuzzleConfig::Map(c) => format!(
            "Current: {}×{} · {} regions · {:?}",
            c.dimensions().0,
            c.dimensions().1,
            c.regions,
            c.difficulty
        ),
        p::PuzzleConfig::Untangle { points } => format!("Current: {points} points"),
    });
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
    // Root application exit can lose a retained game even while its surface is
    // suspended. Detached-window close retains this global game and follows its
    // existing terminal reattach/close workflow instead.
    pub(in crate::ui) fn intercept_game_exit(&mut self, ctx: &Context) -> bool {
        if ctx.viewport_id() != ViewportId::ROOT
            || self.games.play.session.is_none()
            || !self.games.play.files.dirty()
        {
            return false;
        }
        let play = &mut self.games.play;
        play.suspended = false;
        if !matches!(play.files.pending, Some(FileAction::Exit)) {
            play.files.invalidate();
            play.files.pending = Some(FileAction::Exit);
        }
        self.allow_session_exit = false;
        self.allow_remote_exit = false;
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
        true
    }
    pub(in crate::ui) fn continue_window_close(&mut self, ctx: &Context) {
        if self.remote_documents_block_close(None) {
            self.begin_ftp_document_close(None);
            self.pending_remote_exit = true;
            self.allow_session_exit = false;
            self.allow_remote_exit = false;
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        } else {
            self.request_window_session_close(ctx);
        }
    }
    pub(in crate::ui) fn games_menu(&mut self, ui: &mut Ui) {
        for spec in GameRegistry::catalog() {
            if ui.button(spec.label).clicked() {
                self.games
                    .play
                    .request_file_action(FileAction::Start(spec.id, 1), ui.ctx());
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
        if current == ViewportId::ROOT && ui.ctx().input(|i| i.viewport().close_requested()) {
            self.intercept_game_exit(ui.ctx());
        }
        let pending_session_decision =
            self.pending_session_close.is_some() || self.pending_paste.is_some();
        let play = &mut self.games.play;
        if current == ViewportId::ROOT {
            if let Some(session) = play.files.poll(&mut play.message) {
                play.adopt_file(session, ui.ctx());
            }
        }
        if play.is_open()
            && ui.ctx().input(|i| i.viewport().close_requested())
            && !(current == ViewportId::ROOT
                && matches!(play.files.pending, Some(FileAction::Exit)))
        {
            play.suspended = true;
            play.files.invalidate();
            play.files.pending = None;
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
        if pending_session_decision {
            return true;
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
        // An editor may release focus while consuming Enter. Remember ownership
        // before widgets run as well as after, so that finishing an edit cannot
        // submit a Guess or trigger a board shortcut in the same frame.
        let solve_owned_keys = play.solve_confirm;
        let widgets_owned_keys = egui::Popup::is_any_open(ui.ctx())
            || ui
                .ctx()
                .memory(|m| m.focused().is_some_and(|id| Some(id) != play.board_focus));
        let mut close = false;
        ScrollArea::vertical().id_salt("games_surface").show(ui,|ui|{Frame::new().fill(content(ui)).inner_margin(16.).show(ui,|ui|{
            if play.files.confirming(){ui.disable();}
            ui.horizontal_wrapped(|ui|{
                ui.image((self.logo.id(),vec2(28.,28.)));
                ui.heading(fluent::emphasized(ui.ctx(), RichText::new(play.session.as_ref().filter(|_|!play.launcher).map_or("Games",|s|s.spec().label))));
                if ui.button("Close (Esc)").clicked(){close=true;}
                if ui.add_enabled(play.session.is_some()&&!play.files.busy(),egui::Button::new("Save…")).clicked(){play.files.start(play.session.clone(),ui.ctx().clone());}
                if ui.add_enabled(!play.files.busy(),egui::Button::new("Load…")).clicked(){play.request_file_action(FileAction::Load,ui.ctx());}
                if play.files.busy(){ui.spinner();ui.label("Game file in progress…");}
                if play.files.dirty(){ui.label("Unsaved changes");}
                if play.session.is_some()&&ui.button(if play.launcher{"Resume game"}else{"Library"}).clicked(){play.launcher=!play.launcher;}
            });
            if play.launcher {
                ui.label("Choose a game. Terminal sessions continue in the background.");
                for spec in GameRegistry::catalog(){if ui.button(spec.label).clicked(){play.request_file_action(FileAction::Start(spec.id,1),ui.ctx());}}
            }else if let Some(s)=&play.session {
                let(id,seed,outcome)=(s.id(),s.seed(),s.outcome());
                ui.horizontal_wrapped(|ui|{
                    if ui.button("New game").clicked(){play.request_file_action(FileAction::New,ui.ctx());}
                    if ui.button("Reset").clicked(){play.request_file_action(FileAction::Reset,ui.ctx());}
                    if ui.add_enabled(play.session.as_ref().unwrap().can_undo(),egui::Button::new("Undo")).clicked(){if play.session.as_mut().unwrap().undo(){play.files.changed();}}
                    if ui.add_enabled(play.session.as_ref().unwrap().can_redo(),egui::Button::new("Redo")).clicked(){if play.session.as_mut().unwrap().redo(){play.files.changed();}}
                    if play.session.as_ref().unwrap().configuration().is_some() && ui.button("Solve…").clicked(){play.solve_confirm=true;}
                    ui.toggle_value(&mut play.help,"Help");
                    ui.label(format!("Seed {seed} · {outcome:?}"));
                });
                if play.session.as_ref().unwrap().assisted(){ui.strong("Assisted — solution revealed");}
                if play.solve_confirm {fluent::surface(ui).show(ui, |ui| {ui.horizontal_wrapped(|ui|{ui.label(if id==GameId::Guess {"Give up and reveal the code? This marks the game assisted."}else{"Reveal the solution? This marks the game assisted."});if ui.button("Reveal solution").clicked(){match play.session.as_mut().unwrap().solve(){Ok(changed)=>{if changed{play.files.changed();}play.message.clear();},Err(e)=>play.message=player_error(e)};play.solve_confirm=false;}let safe = ui.button("Cancel"); fluent::focus_on_entry(ui,&safe,ui.id()); if safe.clicked() || (!play.files.confirming() && ui.ctx().memory(|m|m.top_modal_layer().is_none()) && ui.input_mut(|i|i.consume_key(Modifiers::NONE,Key::Escape))) {play.solve_confirm=false;}});});}
                puzzle_options(ui,play);
                ui.horizontal_wrapped(|ui|{match id {
                    GameId::Soliterm=>{ui.label("New game variant:");ui.selectable_value(&mut play.variant,0,"Freecell");ui.selectable_value(&mut play.variant,1,"Klondike");ui.add(egui::Slider::new(&mut play.passes,1..=20).text("passes"));},
                    GameId::Nimesweeper=>{ui.label("New game preset:");for(i,n)in ["Newbie","Easy","Medium","Hard"].iter().enumerate(){ui.selectable_value(&mut play.preset,i,*n);}},
                    GameId::Termwalk=>{ui.label("New game size:");for size in [3,5,7,9,11,13,15]{ui.selectable_value(&mut play.walk_size,size,format!("{size}×{size}"));}},
_=>{}}});
                if play.help {ui.label(play.session.as_ref().unwrap().help());ui.label("Arrows select a square. Escape closes; Ctrl+Z undoes; Ctrl+Y redoes. Solo: A–G enter 10–16; Guess: A enters colour 10. Untangle: Tab selects a point, arrows move it. Cards: arrows select a pile, Space selects source/destination, D draws. Buttons below expose each move. Variant selections apply to New game.");ui.label("Your current game stays open if new settings cannot create a board.");}
                if matches!(id,GameId::Solo|GameId::Map){ui.checkbox(&mut play.pencil,"Pencil marks");}
                if matches!(id,GameId::Solo|GameId::Sudoku|GameId::Guess|GameId::Map){ui.horizontal_wrapped(|ui|{let max=match play.session.as_ref().unwrap().scene(){GameScene::Puzzle(p::PuzzleScene::Guess(s))=>s.colours,GameScene::Puzzle(p::PuzzleScene::Solo(s))=>s.width as u8,_=>if id==GameId::Map{4}else{9}};for n in 0..=max{if ui.button(if n==0 {"Clear / blank".into()}else{n.to_string()}).clicked(){play.number(n)}}});}
                controls(ui,play,id);
                ScrollArea::both().show(ui,|ui|{draw_scene(ui,play);});
                if outcome!=GameOutcome::Playing{ui.strong("Game finished — Undo, Reset or New game to continue.");}
                if !play.message.is_empty(){ui.label(RichText::new(&play.message).color(ui.visuals().warn_fg_color));}
            }
        });});
        // Let focused controls and popups consume their events first. Only the
        // board (or an unfocused game surface) receives remaining gameplay keys.
        let widgets_own_keys = egui::Popup::is_any_open(ui.ctx())
            || ui
                .ctx()
                .memory(|m| m.focused().is_some_and(|id| Some(id) != play.board_focus));
        if !solve_owned_keys
            && !play.solve_confirm
            && !widgets_owned_keys
            && !widgets_own_keys
            && !play.files.confirming()
        {
            let keys = ui.ctx().input(|i| i.events.clone());
            for event in keys {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if key == Key::Tab
                        && play
                            .session
                            .as_ref()
                            .is_none_or(|s| s.id() != GameId::Untangle)
                    {
                        continue;
                    }
                    if key == Key::Escape {
                        close = true;
                    } else if !play.launcher {
                        play.key(key, modifiers);
                    }
                    // Arrow/Tab board navigation must not also move egui focus.
                    ui.ctx()
                        .memory_mut(|m| m.move_focus(egui::FocusDirection::None));
                }
            }
        }
        if !play.launcher
            && !play.files.confirming()
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
            play.request_file_action(FileAction::Close, ui.ctx());
        }
        play.file_confirmation(ui.ctx());
        let exit_ready = std::mem::take(&mut play.exit_ready);
        if exit_ready {
            self.continue_window_close(ui.ctx());
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
                    slot: play.selected.min(play.dimensions().0 - 1),
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
        7 => Color32::from_rgb(100, 210, 220),
        8 => Color32::from_rgb(235, 225, 200),
        9 => Color32::from_rgb(140, 155, 70),
        10 => Color32::from_rgb(180, 140, 110),
        _ => Color32::from_gray(55),
    }
}
fn board_keyboard_focus(ui: &Ui, play: &mut PlayState, response: &egui::Response) {
    play.board_focus = Some(response.id);
    if response.clicked()
        || response.secondary_clicked()
        || response.middle_clicked()
        || response.drag_started()
    {
        response.request_focus();
    }
    ui.ctx().memory_mut(|m| {
        m.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                tab: play
                    .session
                    .as_ref()
                    .is_some_and(|s| s.id() == GameId::Untangle),
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });
}
fn board(
    ui: &mut Ui,
    play: &mut PlayState,
    w: usize,
    h: usize,
    labels: &[String],
    colours: &[Color32],
) -> Option<(usize, PointerButton)> {
    board_with_minimum(ui, play, (w, h), labels, colours, 24.)
}
fn board_with_minimum(
    ui: &mut Ui,
    play: &mut PlayState,
    dimensions: (usize, usize),
    labels: &[String],
    colours: &[Color32],
    minimum: f32,
) -> Option<(usize, PointerButton)> {
    let (w, h) = dimensions;
    let size = (ui.available_width().min(880.) / w as f32).clamp(minimum, 38.);
    let (rect, response) =
        ui.allocate_exact_size(vec2(size * w as f32, size * h as f32), egui::Sense::click());
    play.board = Some(rect);
    board_keyboard_focus(ui, play, &response);
    for i in 0..w * h {
        let r = Rect::from_min_size(
            rect.min + vec2((i % w) as f32 * size, (i / w) as f32 * size),
            vec2(size, size),
        );
        let fill = colours.get(i).copied().unwrap_or(palette(0));
        ui.painter().rect_filled(r.shrink(1.), 2., fill);
        let linear = egui::Rgba::from(fill);
        let luminance = 0.2126 * linear.r() + 0.7152 * linear.g() + 0.0722 * linear.b();
        // Equal black and white WCAG contrast occurs at sqrt(.0525) - .05.
        let foreground = if luminance > 0.179 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        if let Some(label) = labels.get(i) {
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::monospace((size * 0.50).max(9.)),
                foreground,
            );
        }
        if i == play.selected {
            ui.painter().rect_stroke(
                r.shrink(1.),
                1.,
                Stroke::new(2., foreground),
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
            ui.label(if s.wrapping {
                "Opposite edges wrap ↔ ↕ · orange lines are barriers"
            } else {
                "Closed edges · orange lines are barriers"
            });
            let hit = pipes(
                ui,
                play,
                s.width,
                s.height,
                &s.tiles,
                &s.connected,
                Some(&s.locked),
                s.source,
            );
            let r = play.board.unwrap();
            let size = r.width() / s.width as f32;
            for (i, &mask) in s.barriers.iter().enumerate() {
                let min = r.min + vec2((i % s.width) as f32 * size, (i / s.width) as f32 * size);
                for (bit, a, b) in [
                    (1, vec2(0., 0.), vec2(size, 0.)),
                    (2, vec2(size, 0.), vec2(size, size)),
                    (4, vec2(0., size), vec2(size, size)),
                    (8, vec2(0., 0.), vec2(0., size)),
                ] {
                    if mask & bit != 0 {
                        ui.painter()
                            .line_segment([min + a, min + b], Stroke::new(3., palette(2)));
                    }
                }
                if s.wrapping {
                    let (x, y) = (i % s.width, i / s.width);
                    for (edge, bit, delta, label) in [
                        (y == 0, 1, vec2(size / 2., 3.), "↑"),
                        (x + 1 == s.width, 2, vec2(size - 3., size / 2.), "→"),
                        (y + 1 == s.height, 4, vec2(size / 2., size - 3.), "↓"),
                        (x == 0, 8, vec2(3., size / 2.), "←"),
                    ] {
                        if edge && s.tiles[i] & bit != 0 && mask & bit == 0 {
                            ui.painter().text(
                                min + delta,
                                Align2::CENTER_CENTER,
                                label,
                                FontId::monospace(10.),
                                palette(5),
                            );
                        }
                    }
                }
            }
            if let Some((cell, b)) = hit {
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
            ui.label(match s.generation {
                p::MinesGeneration::Unplaced => "Mines are placed after your first reveal.",
                p::MinesGeneration::Randomized => {
                    if s.solvable {
                        "No-guess solution verified."
                    } else {
                        "Guessing may be necessary."
                    }
                }
                p::MinesGeneration::CertifiedFallback => {
                    "No-guess starting board · alternative layout used."
                }
            });
            ui.label(format!(
                "{} mines · {} flags · {} deaths",
                s.mine_count, s.flags, s.deaths
            ));
            let labels = s
                .cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if s.solution.as_ref().is_some_and(|v| v[i]) {
                        "✹".into()
                    } else {
                        match c {
                            p::MineCell::Covered => "·".into(),
                            p::MineCell::Flagged => "⚑".into(),
                            p::MineCell::Open(0) => "".into(),
                            p::MineCell::Open(n) => n.to_string(),
                            p::MineCell::Exploded => "✹".into(),
                        }
                    }
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
                .map(|&d| {
                    if d != 0 && s.cages.is_empty() {
                        d.to_string()
                    } else {
                        String::new()
                    }
                })
                .collect::<Vec<_>>();
            let colours = (0..s.digits.len())
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
            // Cages need a top strip for sums and room for pencil notes.
            // The surrounding scroll area keeps this legible on narrow views.
            board_with_minimum(
                ui,
                play,
                (s.width, s.height),
                &labels,
                &colours,
                if s.cages.is_empty() { 24. } else { 38. },
            );
            let r = play.board.unwrap();
            let size = r.width() / s.width as f32;
            if !s.cages.is_empty() {
                for (i, &digit) in s.digits.iter().enumerate() {
                    if digit != 0 {
                        let pos = r.min
                            + vec2(
                                (i % s.width) as f32 * size + size * 0.5,
                                (i / s.width) as f32 * size + size * 0.65,
                            );
                        ui.painter().text(
                            pos,
                            Align2::CENTER_CENTER,
                            digit.to_string(),
                            FontId::monospace(size * 0.44),
                            Color32::WHITE,
                        );
                    }
                }
            }
            let cols = (s.width as f32).sqrt().ceil() as usize;
            for (i, &bits) in s.pencil.iter().enumerate() {
                if s.digits[i] != 0 {
                    continue;
                }
                for n in 1..=s.width {
                    if bits & (1u32 << n) != 0 {
                        let slot = n - 1;
                        let pos = r.min
                            + vec2((i % s.width) as f32 * size, (i / s.width) as f32 * size)
                            + if s.cages.is_empty() {
                                vec2((slot % cols) as f32 + 0.5, (slot / cols) as f32 + 0.5)
                                    * (size / cols as f32)
                            } else {
                                vec2(4., 14.)
                                    + vec2(
                                        ((slot % cols) as f32 + 0.5) * ((size - 8.) / cols as f32),
                                        ((slot / cols) as f32 + 0.5) * ((size - 16.) / cols as f32),
                                    )
                            };
                        ui.painter().text(
                            pos,
                            Align2::CENTER_CENTER,
                            n.to_string(),
                            FontId::monospace(if s.cages.is_empty() {
                                size / cols as f32 * 0.7
                            } else {
                                (size - 16.) / cols as f32 * 0.85
                            }),
                            Color32::WHITE,
                        );
                    }
                }
            }
            for y in 0..s.height {
                for x in 0..s.width {
                    let i = y * s.width + x;
                    let min = r.min + vec2(x as f32 * size, y as f32 * size);
                    let edge = |a, b| {
                        ui.painter()
                            .line_segment([min + a, min + b], Stroke::new(2., Color32::WHITE));
                    };
                    if y == 0 {
                        edge(vec2(0., 0.), vec2(size, 0.));
                    }
                    if x == 0 {
                        edge(vec2(0., 0.), vec2(0., size));
                    }
                    if x + 1 == s.width || s.regions[i] != s.regions[i + 1] {
                        edge(vec2(size, 0.), vec2(size, size));
                    }
                    if y + 1 == s.height || s.regions[i] != s.regions[i + s.width] {
                        edge(vec2(0., size), vec2(size, size));
                    }
                }
            }
            for cage in &s.cages {
                for &i in &cage.cells {
                    let (x, y) = (i % s.width, i / s.width);
                    let min = r.min + vec2(x as f32 * size, y as f32 * size);
                    let top = y == 0 || !cage.cells.contains(&i.saturating_sub(s.width));
                    let right = x + 1 == s.width || !cage.cells.contains(&(i + 1));
                    let bottom = y + 1 == s.height || !cage.cells.contains(&(i + s.width));
                    let left = x == 0 || !cage.cells.contains(&i.saturating_sub(1));
                    let inset = 4.;
                    // Offset each contour edge inward. Concave corners extend
                    // into the neighbouring cell so every cage stays closed.
                    let has = |dx: isize, dy: isize| {
                        let (cx, cy) = (x as isize + dx, y as isize + dy);
                        cx >= 0
                            && cy >= 0
                            && cx < s.width as isize
                            && cy < s.height as isize
                            && cage.cells.contains(&(cy as usize * s.width + cx as usize))
                    };
                    let start = |convex, concave| {
                        if convex {
                            inset
                        } else if concave {
                            -inset
                        } else {
                            0.
                        }
                    };
                    let end = |convex, concave| {
                        if convex {
                            size - inset
                        } else if concave {
                            size + inset
                        } else {
                            size
                        }
                    };
                    for (visible, a, b) in [
                        (
                            top,
                            vec2(start(left, has(-1, -1)), inset),
                            vec2(end(right, has(1, -1)), inset),
                        ),
                        (
                            right,
                            vec2(size - inset, start(top, has(1, -1))),
                            vec2(size - inset, end(bottom, has(1, 1))),
                        ),
                        (
                            bottom,
                            vec2(start(left, has(-1, 1)), size - inset),
                            vec2(end(right, has(1, 1)), size - inset),
                        ),
                        (
                            left,
                            vec2(inset, start(top, has(-1, -1))),
                            vec2(inset, end(bottom, has(-1, 1))),
                        ),
                    ] {
                        if visible {
                            ui.painter()
                                .line_segment([min + a, min + b], Stroke::new(1.2, palette(2)));
                        }
                    }
                }
                if let Some(&anchor) = cage.cells.iter().min() {
                    let pos = r.min
                        + vec2(
                            (anchor % s.width) as f32 * size + 6.,
                            (anchor / s.width) as f32 * size + 5.,
                        );
                    ui.painter().text(
                        pos,
                        Align2::LEFT_TOP,
                        cage.sum.to_string(),
                        FontId::monospace(9.),
                        palette(2),
                    );
                }
            }
            if s.diagonal {
                for i in 0..s.width * 2 {
                    let a = i as f32 / (s.width * 2) as f32;
                    let z = (i as f32 + 0.5) / (s.width * 2) as f32;
                    for flip in [false, true] {
                        let point = |t: f32| {
                            r.min + vec2(r.width() * t, r.height() * if flip { 1. - t } else { t })
                        };
                        ui.painter()
                            .line_segment([point(a), point(z)], Stroke::new(1.5, palette(5)));
                    }
                }
            }
            let topology = match play.session.as_ref().and_then(GameSession::configuration) {
                Some(p::PuzzleConfig::SoloVariant(c)) => solo_variant_name(c),
                _ => "Regular",
            };
            ui.label(format!(
                "{topology} Solo{}",
                if s.diagonal {
                    " · both long diagonals"
                } else {
                    ""
                }
            ));
            if !s.cages.is_empty() {
                ui.label("White outlines mark regions. Orange cages contain distinct digits adding to their small sum.");
            }
            ui.label(format!(
                "Difficulty: {:?}{} · {}",
                s.certificate.achieved,
                if s.certificate.easier_fallback {
                    " (simpler than requested)"
                } else {
                    ""
                },
                symmetry_name(s.symmetry)
            ));
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
            ui.label(format!(
                "{} of {} guesses · {} colours",
                s.rows.len(),
                s.max_guesses,
                s.colours
            ));
            if let Some(code) = &s.solution {
                ui.label(format!(
                    "Revealed code: {}",
                    code.iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
            for (i, row) in s.rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", i + 1));
                    for &c in &row.pegs {
                        ui.colored_label(palette(c), format!("●{c}"));
                    }
                    ui.label(format!("{} exact · {} elsewhere", row.exact, row.misplaced));
                });
            }
            let labels = (0..s.current.len())
                .map(|i| format!("{}{}", s.current[i], if s.held[i] { "◆" } else { "" }))
                .collect::<Vec<_>>();
            let colours = s.current.iter().map(|&c| palette(c)).collect::<Vec<_>>();
            if let Some((slot, b)) = board(ui, play, s.current.len(), 1, &labels, &colours) {
                if b == PointerButton::Secondary {
                    play.puzzle(p::PuzzleInput::Hold { slot })
                } else {
                    play.puzzle(p::PuzzleInput::Peg {
                        slot,
                        colour: (s.current[slot] + 1) % (s.colours + 1),
                    })
                }
            }
        }
        GameScene::Puzzle(p::PuzzleScene::Map(s)) => {
            ui.label(format!(
                "Difficulty: {:?}{} · {}",
                s.certificate.achieved,
                if s.configuration.difficulty != s.certificate.achieved {
                    " (simpler than requested)"
                } else {
                    ""
                },
                if s.generation == p::MapGeneration::CertifiedFallback {
                    "alternative starting board"
                } else {
                    "new starting board"
                }
            ));
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
            board_keyboard_focus(ui, play, &response);
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
        w.games.play.puzzle(p::PuzzleInput::Flag { cell: 0 });
        assert!(w.games.play.files.dirty());
        let saved = w.games.play.session.as_ref().unwrap().scene();
        let once = Arc::new(AtomicBool::new(true));
        let seen = Arc::new(AtomicBool::new(false));
        let visible = seen.clone();
        ctx.set_embed_viewports(false);
        Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let mut raw = RawInput {
                viewport_id: viewport.ids.this,
                events: vec![Event::Text("blocked".into())],
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
        let mut out = ctx.run_ui(raw.clone(), |ui| w.detached_windows(ui.ctx()));
        assert!(
            out.viewport_output[&child]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        out.textures_delta.clear();
        ctx.run_ui(raw, |ui| w.detached_windows(ui.ctx()))
            .textures_delta
            .clear();
        assert!(seen.load(Ordering::SeqCst));
        w.tabs[0].poll();
        assert!(crate::ui::terminal_review_tests::captured(&w.tabs[0]).is_empty());
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

#[cfg(all(test, unix))]
mod options_ui_tests {
    use super::*;
    #[test]
    fn options_dynamic_guess_last_slot_and_atomic_failure() {
        let mut s = PlayState::default();
        let c = p::PuzzleConfig::Guess(p::GuessConfig {
            colours: 8,
            pegs: 5,
            max_guesses: 12,
            allow_blank: true,
            allow_multiple: false,
        });
        assert!(s.start_config(9, c));
        assert_eq!(s.dimensions(), (5, 1));
        s.selected = 4;
        s.number(8);
        let old = s.session.as_ref().unwrap().scene();
        assert!(!s.start_config(10, p::PuzzleConfig::Untangle { points: 100 }));
        assert_eq!(s.session.as_ref().unwrap().scene(), old);
        assert_eq!(s.selected, 4);
        let GameScene::Puzzle(p::PuzzleScene::Guess(g)) = old else {
            panic!()
        };
        assert_eq!(g.current, vec![0, 0, 0, 0, 8]);
    }
    fn frame(ctx: &Context, w: &mut Workspace, events: Vec<Event>, size: Vec2) {
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                assert!(w.games_frame(ui));
            },
        );
        out.textures_delta.clear();
    }
    fn click(ctx: &Context, w: &mut Workspace, pos: Pos2, button: PointerButton, size: Vec2) {
        frame(
            ctx,
            w,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            size,
        );
        frame(
            ctx,
            w,
            vec![Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
            size,
        );
    }
    #[test]
    fn options_actual_super_guess_fifth_peg_click_and_hold() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let c = p::PuzzleConfig::Guess(p::GuessConfig {
            colours: 8,
            pegs: 5,
            max_guesses: 12,
            ..Default::default()
        });
        w.games.play.start_config(19, c);
        let size = vec2(1000., 900.);
        frame(&ctx, &mut w, vec![], size);
        frame(&ctx, &mut w, vec![], size);
        let r = w.games.play.board.unwrap();
        let pos = r.min + vec2(r.width() * 0.9, r.height() * 0.5);
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let mut expected = GameSession::new_puzzle(19, c).unwrap();
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 4,
                colour: 1,
            }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        click(&ctx, &mut w, pos, PointerButton::Secondary, size);
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Hold { slot: 4 }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
    }
    #[test]
    fn options_actual_wrapping_net_last_cell_and_solve_undo_redo() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let c = p::PuzzleConfig::Net(p::NetConfig {
            width: 7,
            height: 5,
            wrapping: true,
            barrier_percent: 25,
            unique: false,
        });
        w.games.play.start_config(9, c);
        let size = vec2(1000., 900.);
        frame(&ctx, &mut w, vec![], size);
        frame(&ctx, &mut w, vec![], size);
        let r = w.games.play.board.unwrap();
        let pos = r.min + vec2(r.width() * (6.5 / 7.), r.height() * 0.9);
        click(&ctx, &mut w, pos, PointerButton::Middle, size);
        let mut expected = GameSession::new_puzzle(9, c).unwrap();
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Lock { cell: 34 }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        let before = expected.scene();
        w.games.play.session.as_mut().unwrap().solve().unwrap();
        assert!(w.games.play.session.as_ref().unwrap().assisted());
        w.games.play.key(Key::Z, Modifiers::CTRL);
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        w.games.play.key(Key::Y, Modifiers::CTRL);
        assert!(w.games.play.session.as_ref().unwrap().assisted());
        let GameScene::Puzzle(p::PuzzleScene::Net(n)) =
            w.games.play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert!(n.locked.iter().all(|&v| v));
        assert!(n.wrapping);
        assert!(n.barriers.iter().any(|&v| v != 0));
    }
    #[test]
    fn options_changed_sizes_render_inside_scrolling_surface() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        for blocks in [(2, 2), (2, 3), (3, 2), (3, 3), (4, 3), (4, 4)] {
            w.games.play.start_config(
                4,
                p::PuzzleConfig::Solo(p::SoloConfig {
                    block_width: blocks.0,
                    block_height: blocks.1,
                    ..Default::default()
                }),
            );
            for size in [vec2(340., 420.), vec2(1400., 1100.)] {
                frame(&ctx, &mut w, vec![], size);
                let r = w.games.play.board.unwrap();
                assert!(r.is_finite());
                assert_eq!(r.width(), r.height());
                assert_eq!(
                    w.games.play.dimensions(),
                    (blocks.0 * blocks.1, blocks.0 * blocks.1)
                );
            }
        }
    }
    fn label_position(ctx: &Context, w: &mut Workspace, label: &str) -> Pos2 {
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1000.))),
                ..Default::default()
            },
            |ui| {
                w.games_frame(ui);
            },
        );
        fn text_pos(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| text_pos(s, label)),
                _ => None,
            }
        }
        let p = out.shapes.iter().find_map(|s| text_pos(&s.shape, label));
        out.textures_delta.clear();
        p.unwrap_or_else(|| panic!("missing visible control {label}"))
    }
    #[test]
    fn options_actual_solve_confirmation_and_redo_buttons() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        let size = vec2(1200., 1000.);
        frame(&ctx, &mut w, vec![], size);
        let pos = label_position(&ctx, &mut w, "Solve…");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert!(!w.games.play.session.as_ref().unwrap().assisted());
        let pos = label_position(&ctx, &mut w, "Reveal solution");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert!(w.games.play.session.as_ref().unwrap().assisted());
        let pos = label_position(&ctx, &mut w, "Undo");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert!(!w.games.play.session.as_ref().unwrap().assisted());
        let pos = label_position(&ctx, &mut w, "Redo");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert!(w.games.play.session.as_ref().unwrap().assisted());
    }
    #[test]
    fn options_actual_new_game_applies_draft_then_rejects_bad_settings() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        let size = vec2(1200., 1000.);
        frame(&ctx, &mut w, vec![], size);
        let config = p::PuzzleConfig::Guess(p::GuessConfig {
            colours: 8,
            pegs: 5,
            max_guesses: 12,
            ..Default::default()
        });
        w.games.play.draft = Some(config);
        let pos = label_position(&ctx, &mut w, "New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().configuration(),
            Some(config)
        );
        assert_eq!(w.games.play.session.as_ref().unwrap().seed(), 6);
        w.games.play.selected = 4;
        w.games.play.number(8);
        let before = w.games.play.session.as_ref().unwrap().scene();
        w.games.play.draft = Some(p::PuzzleConfig::Guess(p::GuessConfig {
            pegs: 100,
            ..Default::default()
        }));
        let pos = label_position(&ctx, &mut w, "New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert!(w.games.play.files.pending.is_some());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        save_ui_click(&ctx, &mut w, "Discard changes and continue");
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().configuration(),
            Some(config)
        );
        assert_eq!(w.games.play.selected, 4);
        assert!(w.games.play.session.as_ref().unwrap().can_undo());
        assert!(!w.games.play.message.is_empty());
    }
    #[test]
    fn options_actual_preset_menu_selects_super_without_replacing_board() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        let size = vec2(1200., 1000.);
        frame(&ctx, &mut w, vec![], size);
        let old = w.games.play.session.as_ref().unwrap().scene();
        let pos = label_position(&ctx, &mut w, "Puzzle options — apply with New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let pos = label_position(&ctx, &mut w, "Choose preset…");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let pos = label_position(&ctx, &mut w, "Super");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), old);
        assert!(matches!(
            w.games.play.draft,
            Some(p::PuzzleConfig::Guess(p::GuessConfig {
                pegs: 5,
                colours: 8,
                max_guesses: 12,
                ..
            }))
        ));
        let pos = label_position(&ctx, &mut w, "New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert_eq!(w.games.play.dimensions(), (5, 1));
    }
    #[test]
    fn options_custom_guess_blanks_distinct_and_failed_submission_keeps_row() {
        let mut s = PlayState::default();
        let c = p::PuzzleConfig::Guess(p::GuessConfig {
            colours: 8,
            pegs: 5,
            max_guesses: 12,
            allow_blank: true,
            allow_multiple: false,
        });
        s.start_config(7, c);
        s.selected = 0;
        s.number(8);
        s.selected = 1;
        s.number(8);
        let before = s.session.as_ref().unwrap().scene();
        s.key(Key::Enter, Modifiers::default());
        assert_eq!(s.session.as_ref().unwrap().scene(), before);
        assert!(!s.message.is_empty());
        s.number(0);
        s.key(Key::Enter, Modifiers::default());
        let GameScene::Puzzle(p::PuzzleScene::Guess(g)) = s.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert_eq!(g.rows.len(), 1);
        assert_eq!(g.rows[0].pegs, vec![8, 0, 0, 0, 0]);
    }
    #[test]
    fn options_large_mines_and_map_pointer_coordinates() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let size = vec2(1400., 1600.);
        let c = p::PuzzleConfig::Mines(p::MinesConfig {
            width: 30,
            height: 16,
            mines: 170,
            solvable: true,
        });
        assert!(w.games.play.start_config(2, c));
        frame(&ctx, &mut w, vec![], size);
        frame(&ctx, &mut w, vec![], size);
        let r = w.games.play.board.unwrap();
        let pos = r.min + vec2(r.width() * (29.5 / 30.), r.height() * (15.5 / 16.));
        click(&ctx, &mut w, pos, PointerButton::Secondary, size);
        let mut expected = GameSession::new_puzzle(2, c).unwrap();
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Flag { cell: 479 }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        let c = p::PuzzleConfig::Map(p::MapConfig {
            preset: p::MapPreset::Portrait,
            regions: 75,
            difficulty: p::MapDifficulty::Normal,
        });
        assert!(w.games.play.start_config(2, c));
        frame(&ctx, &mut w, vec![], size);
        frame(&ctx, &mut w, vec![], size);
        let GameScene::Puzzle(p::PuzzleScene::Map(g)) =
            w.games.play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert_eq!((g.width, g.height), (25, 30));
        let cell = (0..g.regions.len())
            .rev()
            .find(|&i| !g.givens[g.regions[i]])
            .unwrap();
        let r = w.games.play.board.unwrap();
        let pos = r.min
            + vec2(
                r.width() * ((cell % g.width) as f32 + 0.5) / g.width as f32,
                r.height() * ((cell / g.width) as f32 + 0.5) / g.height as f32,
            );
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let mut expected = GameSession::new_puzzle(2, c).unwrap();
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Colour {
                region: g.regions[cell],
                colour: 1,
            }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        for c in [
            p::PuzzleConfig::Mines(p::MinesConfig {
                width: 3,
                height: 4,
                mines: 1,
                solvable: false,
            }),
            p::PuzzleConfig::Untangle { points: 6 },
            p::PuzzleConfig::Untangle { points: 25 },
        ] {
            assert!(w.games.play.start_config(2, c));
            frame(&ctx, &mut w, vec![], vec2(340., 420.));
            assert!(w.games.play.board.unwrap().is_finite());
        }
    }
    #[test]
    fn options_net_barriers_and_solo_bit16_are_painted_from_scene() {
        let ctx = Context::default();
        let mut s = PlayState::default();
        s.start_config(
            9,
            p::PuzzleConfig::Net(p::NetConfig {
                width: 7,
                height: 5,
                wrapping: true,
                barrier_percent: 25,
                unique: false,
            }),
        );
        let GameScene::Puzzle(p::PuzzleScene::Net(g)) = s.session.as_ref().unwrap().scene() else {
            panic!()
        };
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 900.))),
                ..Default::default()
            },
            |ui| draw_scene(ui, &mut s),
        );
        let r = s.board.unwrap();
        let size = r.width() / 7.;
        let segments = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::epaint::Shape::LineSegment { points, stroke }
                    if stroke.color == palette(2) && stroke.width == 3. =>
                {
                    Some(*points)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        out.textures_delta.clear();
        for (i, &mask) in g.barriers.iter().enumerate() {
            let min = r.min + vec2((i % 7) as f32 * size, (i / 7) as f32 * size);
            for (bit, a, b) in [
                (1, vec2(0., 0.), vec2(size, 0.)),
                (2, vec2(size, 0.), vec2(size, size)),
                (4, vec2(0., size), vec2(size, size)),
                (8, vec2(0., 0.), vec2(0., size)),
            ] {
                if mask & bit != 0 {
                    assert!(segments.contains(&[min + a, min + b]));
                }
            }
        }
        assert!(!segments.is_empty());
        out.textures_delta.clear();
        s.start_config(
            3,
            p::PuzzleConfig::Solo(p::SoloConfig {
                block_width: 4,
                block_height: 4,
                ..Default::default()
            }),
        );
        let GameScene::Puzzle(p::PuzzleScene::Solo(g)) = s.session.as_ref().unwrap().scene() else {
            panic!()
        };
        s.selected = g.givens.iter().position(|&v| !v).unwrap();
        s.pencil = true;
        s.number(16);
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000., 900.))),
                ..Default::default()
            },
            |ui| draw_scene(ui, &mut s),
        );
        let r = s.board.unwrap();
        let cell = r.width() / 16.;
        let expected = r.min
            + vec2(
                (s.selected % 16) as f32 * cell,
                (s.selected / 16) as f32 * cell,
            )
            + vec2(3.5, 3.5) * (cell / 4.);
        let painted=out.shapes.iter().any(|c|matches!(&c.shape,egui::epaint::Shape::Text(t) if t.galley.text()=="16" && (t.pos+t.galley.size()*0.5).distance(expected)<0.1));
        out.textures_delta.clear();
        assert!(painted);
    }
    #[test]
    fn options_guess_tenth_colour_keyboard_reaches_last_custom_slot() {
        let mut s = PlayState::default();
        s.start_config(
            2,
            p::PuzzleConfig::Guess(p::GuessConfig {
                colours: 10,
                pegs: 8,
                max_guesses: 32,
                allow_blank: true,
                allow_multiple: false,
            }),
        );
        s.selected = 7;
        s.key(Key::A, Modifiers::default());
        let GameScene::Puzzle(p::PuzzleScene::Guess(g)) = s.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert_eq!(g.current[7], 10);
    }
    fn assert_same_history(actual: &GameSession, expected: &GameSession) {
        let mut actual = actual.clone();
        let mut expected = expected.clone();
        assert_eq!(actual.scene(), expected.scene());
        assert_eq!(actual.assisted(), expected.assisted());
        while expected.can_undo() {
            assert!(actual.undo());
            expected.undo();
            assert_eq!(actual.scene(), expected.scene());
            assert_eq!(actual.assisted(), expected.assisted());
        }
        assert!(!actual.can_undo());
        while expected.can_redo() {
            assert!(actual.redo());
            expected.redo();
            assert_eq!(actual.scene(), expected.scene());
            assert_eq!(actual.assisted(), expected.assisted());
        }
        assert!(!actual.can_redo());
    }
    fn focus_colours_editor(ctx: &Context, w: &mut Workspace) {
        let size = vec2(1200., 1000.);
        frame(ctx, w, vec![], size);
        let pos = label_position(ctx, w, "Puzzle options — apply with New game");
        click(ctx, w, pos, PointerButton::Primary, size);
        for _ in 0..20 {
            frame(ctx, w, vec![], size);
        }
        let pos = label_position(ctx, w, "6");
        click(ctx, w, pos, PointerButton::Primary, size);
        frame(ctx, w, vec![], size);
        assert!(ctx.egui_wants_keyboard_input());
    }
    fn key_event(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    #[test]
    fn options_focused_numeric_edit_owns_digits_shortcuts_and_enter() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        for (slot, n) in [1, 2, 3, 4].into_iter().enumerate() {
            w.games.play.selected = slot;
            w.games.play.number(n);
        }
        w.games.play.number(5);
        assert!(w.games.play.session.as_mut().unwrap().undo());
        let before_session = w.games.play.session.as_ref().unwrap().clone();
        let before = w.games.play.session.as_ref().unwrap().scene();
        let selected = w.games.play.selected;
        assert!(w.games.play.session.as_ref().unwrap().can_undo());
        assert!(w.games.play.session.as_ref().unwrap().can_redo());
        focus_colours_editor(&ctx, &mut w);
        let size = vec2(1200., 1000.);
        frame(
            &ctx,
            &mut w,
            vec![
                key_event(Key::Num3, Modifiers::default()),
                Event::Text("3".into()),
            ],
            size,
        );
        assert!(matches!(
            w.games.play.draft,
            Some(p::PuzzleConfig::Guess(p::GuessConfig { colours: 3, .. }))
        ));
        for events in [
            vec![],
            vec![key_event(Key::Z, Modifiers::CTRL)],
            vec![key_event(Key::Y, Modifiers::CTRL)],
            vec![key_event(Key::ArrowRight, Modifiers::default())],
            vec![key_event(Key::Enter, Modifiers::default())],
        ] {
            frame(&ctx, &mut w, events, size);
            assert_eq!(
                w.games.play.session.as_ref().unwrap().scene(),
                before,
                "focused settings must not make a gameplay move"
            );
            assert_same_history(w.games.play.session.as_ref().unwrap(), &before_session);
            assert_eq!(w.games.play.selected, selected);
            assert!(!w.games.play.session.as_ref().unwrap().assisted());
            assert!(w.games.play.session.as_ref().unwrap().can_undo());
            assert!(w.games.play.session.as_ref().unwrap().can_redo());
            assert!(
                ctx.input(|i| i.events.is_empty()),
                "settings input stays quarantined from terminals"
            );
        }
    }
    #[test]
    fn options_focused_shortcut_preserves_assisted_game() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        w.games.play.session.as_mut().unwrap().solve().unwrap();
        let before = w.games.play.session.as_ref().unwrap().scene();
        focus_colours_editor(&ctx, &mut w);
        frame(
            &ctx,
            &mut w,
            vec![key_event(Key::Z, Modifiers::CTRL)],
            vec2(1200., 1000.),
        );
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        assert!(w.games.play.session.as_ref().unwrap().assisted());
    }
    #[test]
    fn options_leaving_editor_by_board_click_restores_game_keys() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        focus_colours_editor(&ctx, &mut w);
        let size = vec2(1200., 1000.);
        let r = w.games.play.board.unwrap();
        let pos = r.min + vec2(r.width() * 0.625, r.height() * 0.5);
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        frame(
            &ctx,
            &mut w,
            vec![
                key_event(Key::ArrowRight, Modifiers::default()),
                key_event(Key::Num4, Modifiers::default()),
            ],
            size,
        );
        let mut expected = GameSession::new(GameId::Guess, 5);
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 2,
                colour: 1,
            }))
            .unwrap();
        expected
            .input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 3,
                colour: 4,
            }))
            .unwrap();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        frame(&ctx, &mut w, vec![key_event(Key::Z, Modifiers::CTRL)], size);
        expected.undo();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
        frame(&ctx, &mut w, vec![key_event(Key::Y, Modifiers::CTRL)], size);
        expected.redo();
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            expected.scene()
        );
    }
    fn focus_control_by_tab(ctx: &Context, w: &mut Workspace, label: &str) {
        let pos = label_position(ctx, w, label);
        for _ in 0..80 {
            frame(
                ctx,
                w,
                vec![key_event(Key::Tab, Modifiers::default())],
                vec2(1200., 1000.),
            );
            if ctx
                .memory(|m| m.focused())
                .and_then(|id| ctx.read_response(id))
                .is_some_and(|r| r.rect.contains(pos))
            {
                return;
            }
        }
        panic!("could not tab to {label}");
    }
    #[test]
    fn options_widget_buttons_and_combo_receive_keyboard_without_board_moves() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        for (slot, n) in [1, 2, 3, 4].into_iter().enumerate() {
            w.games.play.selected = slot;
            w.games.play.number(n);
        }
        focus_colours_editor(&ctx, &mut w);
        let size = vec2(1200., 1000.);
        let r = w.games.play.board.unwrap();
        click(&ctx, &mut w, r.center(), PointerButton::Secondary, size);
        let before = w.games.play.session.as_ref().unwrap().scene();
        let selected = w.games.play.selected;
        focus_control_by_tab(&ctx, &mut w, "Allow blanks");
        frame(
            &ctx,
            &mut w,
            vec![key_event(Key::Space, Modifiers::default())],
            size,
        );
        assert!(matches!(
            w.games.play.draft,
            Some(p::PuzzleConfig::Guess(p::GuessConfig {
                allow_blank: true,
                ..
            }))
        ));
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        assert_eq!(w.games.play.selected, selected);
        focus_control_by_tab(&ctx, &mut w, "Choose preset…");
        frame(
            &ctx,
            &mut w,
            vec![key_event(Key::Enter, Modifiers::default())],
            size,
        );
        assert!(egui::Popup::is_any_open(&ctx));
        for key in [Key::ArrowDown, Key::Enter] {
            frame(
                &ctx,
                &mut w,
                vec![key_event(key, Modifiers::default())],
                size,
            );
            assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
            assert_eq!(w.games.play.selected, selected);
        }
    }
    #[test]
    fn options_solo_16_keyboard_pencil() {
        let mut s = PlayState::default();
        assert!(s.start_config(
            3,
            p::PuzzleConfig::Solo(p::SoloConfig {
                block_width: 4,
                block_height: 4,
                ..Default::default()
            })
        ));
        assert_eq!(s.dimensions(), (16, 16));
        let GameScene::Puzzle(p::PuzzleScene::Solo(g)) = s.session.as_ref().unwrap().scene() else {
            panic!()
        };
        s.selected = g.givens.iter().position(|&v| !v).unwrap();
        s.pencil = true;
        s.key(Key::G, Modifiers::default());
        let GameScene::Puzzle(p::PuzzleScene::Solo(g)) = s.session.as_ref().unwrap().scene() else {
            panic!()
        };
        assert_eq!(g.pencil[s.selected], 1u32 << 16);
    }
    fn advanced_preset(name: &str) -> p::PuzzleConfig {
        p::PuzzleKind::Solo
            .presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .configuration
            .unwrap()
    }
    fn painted_scene(play: &mut PlayState, width: f32) -> egui::FullOutput {
        let mut out = Context::default().run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1200.))),
                ..Default::default()
            },
            |ui| draw_scene(ui, play),
        );
        out.textures_delta.clear();
        out
    }
    fn painted_lines(out: &egui::FullOutput, stroke: Stroke) -> Vec<[Pos2; 2]> {
        fn collect(s: &egui::epaint::Shape, stroke: Stroke, lines: &mut Vec<[Pos2; 2]>) {
            match s {
                egui::epaint::Shape::LineSegment { points, stroke: st } if *st == stroke => {
                    lines.push(*points)
                }
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        collect(s, stroke, lines);
                    }
                }
                _ => {}
            }
        }
        let mut result = vec![];
        for s in &out.shapes {
            collect(&s.shape, stroke, &mut result);
        }
        result
    }
    fn painted_texts(out: &egui::FullOutput) -> Vec<(Pos2, String)> {
        fn collect(s: &egui::epaint::Shape, texts: &mut Vec<(Pos2, String)>) {
            match s {
                egui::epaint::Shape::Text(t) => texts.push((t.pos, t.galley.text().to_owned())),
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        collect(s, texts);
                    }
                }
                _ => {}
            }
        }
        let mut result = vec![];
        for s in &out.shapes {
            collect(&s.shape, &mut result);
        }
        result
    }
    #[test]
    fn advanced_ui_jigsaw_exact_region_edges_and_diagonals() {
        let mut play = PlayState::default();
        assert!(play.start_config(0, advanced_preset("9 Jigsaw Basic X")));
        let GameScene::Puzzle(p::PuzzleScene::Solo(s)) = play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        let out = painted_scene(&mut play, 430.);
        let r = play.board.unwrap();
        let size = r.width() / 9.;
        let mut expected = vec![];
        let mut differs = false;
        for y in 0..9 {
            for x in 0..9 {
                let i = y * 9 + x;
                let min = r.min + vec2(x as f32 * size, y as f32 * size);
                if y == 0 {
                    expected.push([min, min + vec2(size, 0.)]);
                }
                if x == 0 {
                    expected.push([min, min + vec2(0., size)]);
                }
                if x == 8 || s.regions[i] != s.regions[i + 1] {
                    expected.push([min + vec2(size, 0.), min + vec2(size, size)]);
                }
                if y == 8 || s.regions[i] != s.regions[i + 9] {
                    expected.push([min + vec2(0., size), min + vec2(size, size)]);
                }
                if x < 8 {
                    differs |= (s.regions[i] != s.regions[i + 1]) != ((x + 1) % 3 == 0);
                }
            }
        }
        assert!(
            differs,
            "fixture must distinguish region IDs from rectangular boxes"
        );
        assert_eq!(
            painted_lines(&out, Stroke::new(2., Color32::WHITE)),
            expected
        );
        assert_eq!(painted_lines(&out, Stroke::new(1.5, palette(5))).len(), 36);
        assert!(
            painted_texts(&out)
                .iter()
                .any(|(_, t)| t.contains("Jigsaw"))
        );
        assert!(
            !play
                .session
                .as_ref()
                .unwrap()
                .help()
                .contains("certificate")
        );
        assert!(
            play.session
                .as_ref()
                .unwrap()
                .help()
                .contains("outlined region")
        );
    }
    #[test]
    fn advanced_ui_killer_cages_sums_arithmetic_and_history() {
        let mut play = PlayState::default();
        assert!(play.start_config(0, advanced_preset("3x3 Killer")));
        let GameScene::Puzzle(p::PuzzleScene::Solo(s)) = play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        assert!(s.givens.iter().all(|&v| !v));
        let out = painted_scene(&mut play, 430.);
        let r = play.board.unwrap();
        let size = r.width() / 9.;
        let mut compact_play = PlayState::default();
        assert!(compact_play.start_config(0, advanced_preset("3x3 Killer")));
        let _compact = painted_scene(&mut compact_play, 220.);
        assert!(
            compact_play.board.unwrap().width() / 9. >= 38.,
            "cages retain readable sums and notes through horizontal scrolling"
        );
        let lines = painted_lines(&out, Stroke::new(1.2, palette(2)));
        let texts = painted_texts(&out);
        let mut expected_count = 0;
        for cage in &s.cages {
            assert!(cage.cells.len() > 1);
            let anchor = *cage.cells.iter().min().unwrap();
            let ar = Rect::from_min_size(
                r.min + vec2((anchor % 9) as f32 * size, (anchor / 9) as f32 * size),
                vec2(size, size),
            );
            assert!(
                texts
                    .iter()
                    .any(|(pos, t)| ar.contains(*pos) && *t == cage.sum.to_string()),
                "missing sum at cage anchor {anchor}"
            );
            for &i in &cage.cells {
                let (x, y) = (i % 9, i / 9);
                for (edge, offset, horizontal) in [
                    (
                        y == 0 || !cage.cells.contains(&i.saturating_sub(9)),
                        vec2(size / 2., 4.),
                        true,
                    ),
                    (
                        x == 8 || !cage.cells.contains(&(i + 1)),
                        vec2(size - 4., size / 2.),
                        false,
                    ),
                    (
                        y == 8 || !cage.cells.contains(&(i + 9)),
                        vec2(size / 2., size - 4.),
                        true,
                    ),
                    (
                        x == 0 || !cage.cells.contains(&i.saturating_sub(1)),
                        vec2(4., size / 2.),
                        false,
                    ),
                ] {
                    if edge {
                        expected_count += 1;
                        let point = r.min + vec2(x as f32 * size, y as f32 * size) + offset;
                        let count = lines
                            .iter()
                            .filter(|[a, b]| {
                                if horizontal {
                                    (a.y - point.y).abs() < 0.01
                                        && (b.y - point.y).abs() < 0.01
                                        && point.x >= a.x.min(b.x)
                                        && point.x <= a.x.max(b.x)
                                } else {
                                    (a.x - point.x).abs() < 0.01
                                        && (b.x - point.x).abs() < 0.01
                                        && point.y >= a.y.min(b.y)
                                        && point.y <= a.y.max(b.y)
                                }
                            })
                            .count();
                        assert_eq!(
                            count, 1,
                            "each exposed cage edge has exactly one correctly inset painted segment"
                        );
                    }
                }
            }
        }
        assert_eq!(lines.len(), expected_count);
        let mut endpoints = std::collections::BTreeMap::new();
        for points in &lines {
            for point in points {
                *endpoints
                    .entry((
                        (point.x * 1000.).round() as i32,
                        (point.y * 1000.).round() as i32,
                    ))
                    .or_insert(0) += 1;
            }
        }
        assert!(
            endpoints.values().all(|&degree| degree == 2),
            "every cage outline must close, including concave corners"
        );

        assert!(lines.iter().all(|p| r.contains(p[0]) && r.contains(p[1])));
        assert!(
            play.session
                .as_ref()
                .unwrap()
                .help()
                .contains("distinct digits")
        );
        play.pencil = true;
        play.selected = 80;
        play.key(Key::Num9, Modifiers::default());
        let before = play.session.as_ref().unwrap().scene();
        let GameScene::Puzzle(p::PuzzleScene::Solo(s)) = &before else {
            panic!()
        };
        assert_eq!(s.pencil[80], 1 << 9);
        play.session.as_mut().unwrap().solve().unwrap();
        assert!(play.session.as_ref().unwrap().assisted());
        let GameScene::Puzzle(p::PuzzleScene::Solo(s)) = play.session.as_ref().unwrap().scene()
        else {
            panic!()
        };
        for cage in &s.cages {
            assert_eq!(
                cage.cells
                    .iter()
                    .map(|&i| u16::from(s.digits[i]))
                    .sum::<u16>(),
                cage.sum
            );
            let digits: std::collections::BTreeSet<_> =
                cage.cells.iter().map(|&i| s.digits[i]).collect();
            assert_eq!(digits.len(), cage.cells.len());
        }
        play.key(Key::Z, Modifiers::CTRL);
        assert_eq!(play.session.as_ref().unwrap().scene(), before);
        play.key(Key::Y, Modifiers::CTRL);
        assert!(play.session.as_ref().unwrap().assisted());
        let solved = play.session.as_ref().unwrap().scene();
        let selected = play.selected;
        let invalid = p::PuzzleConfig::SoloVariant(p::SoloVariantConfig {
            base: p::SoloConfig::default(),
            jigsaw: true,
            killer: true,
        });
        assert!(!play.start_config(0, invalid));
        assert_eq!(play.session.as_ref().unwrap().scene(), solved);
        assert_eq!(play.selected, selected);
        assert!(play.session.as_ref().unwrap().assisted());
        let difficult = p::PuzzleConfig::Map(p::MapConfig {
            preset: p::MapPreset::Default,
            regions: 30,
            difficulty: p::MapDifficulty::Unreasonable,
        });
        assert!(difficult.validate().is_ok());
        let failure_seed = (0..=16)
            .find(|&seed| GameSession::new_puzzle(seed, difficult).is_err())
            .expect("bounded independent valid-configuration failure sample");
        assert!(!play.start_config(failure_seed, difficult));
        assert_eq!(play.session.as_ref().unwrap().scene(), solved);
        assert_eq!(play.selected, selected);
        assert_eq!(
            play.session.as_ref().unwrap().configuration(),
            Some(advanced_preset("3x3 Killer"))
        );
        assert!(play.session.as_ref().unwrap().assisted());
        assert!(play.message.contains("Your current game is kept"));
        assert!(!play.message.contains("budget") && !play.message.contains("certificate"));
    }
    #[test]
    fn advanced_ui_custom_flags_regular_help_and_focus_quarantine() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        ctx.all_styles_mut(|s| s.animation_time = 0.);
        assert!(
            w.games
                .play
                .start_config(0, advanced_preset("9 Jigsaw Basic"))
        );
        let size = vec2(1200., 1000.);
        frame(&ctx, &mut w, vec![], size);
        let before = w.games.play.session.as_ref().unwrap().scene();
        let pos = label_position(&ctx, &mut w, "Puzzle options — apply with New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let pos = label_position(&ctx, &mut w, "Jigsaw regions (9×9 only)");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let Some(p::PuzzleConfig::SoloVariant(c)) = w.games.play.draft else {
            panic!()
        };
        assert!(!c.jigsaw && !c.killer);
        assert!(c.validate().is_ok());
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        focus_control_by_tab(&ctx, &mut w, "Killer cages (9×9 only)");
        frame(
            &ctx,
            &mut w,
            vec![
                key_event(Key::Num9, Modifiers::default()),
                key_event(Key::ArrowRight, Modifiers::default()),
                key_event(Key::Z, Modifiers::CTRL),
            ],
            size,
        );
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        let mut play = PlayState::default();
        assert!(play.start_config(0, p::PuzzleConfig::SoloVariant(c)));
        let out = painted_scene(&mut play, 430.);
        let texts = painted_texts(&out);
        assert!(texts.iter().any(|(_, t)| t.contains("Regular")));
        assert!(
            !texts
                .iter()
                .any(|(_, t)| t.contains("Jigsaw") || t.contains("Killer"))
        );
        assert!(play.session.as_ref().unwrap().help().contains("3×3 box"));
    }
    #[test]
    fn advanced_ui_all_51_presets_available_and_15_samples_paint_exact_grade() {
        let kinds = [
            p::PuzzleKind::Net,
            p::PuzzleKind::Mines,
            p::PuzzleKind::Solo,
            p::PuzzleKind::Guess,
            p::PuzzleKind::Map,
            p::PuzzleKind::Untangle,
        ];
        let presets: Vec<_> = kinds.into_iter().flat_map(|k| k.presets()).collect();
        assert_eq!(presets.len(), 51);
        assert!(presets.iter().all(|p| p.configuration.is_some()));
        let names = [
            "3x3 Intermediate",
            "3x3 Advanced",
            "3x3 Advanced X",
            "3x3 Extreme",
            "3x3 Unreasonable",
            "3x3 Killer",
            "9 Jigsaw Basic",
            "9 Jigsaw Basic X",
            "9 Jigsaw Advanced",
        ];
        let mut tested = 0;
        for preset in presets {
            let config = preset.configuration.unwrap();
            let advanced_map = matches!(
                config,
                p::PuzzleConfig::Map(p::MapConfig {
                    difficulty: p::MapDifficulty::Hard | p::MapDifficulty::Unreasonable,
                    ..
                })
            );
            if !names.contains(&preset.name.as_str()) && !advanced_map {
                continue;
            }
            let seed = if matches!(config, p::PuzzleConfig::Map(_)) {
                (0..=32)
                    .find(|&seed| GameSession::new_puzzle(seed, config).is_ok())
                    .unwrap_or_else(|| {
                        panic!(
                            "no independently generated sample for {} within 33 seeds",
                            preset.name
                        )
                    })
            } else {
                0
            };
            let mut play = PlayState::default();
            assert!(
                play.start_config(seed, config),
                "{}: {}",
                preset.name,
                play.message
            );
            let out = painted_scene(&mut play, 460.);
            assert!(play.board.unwrap().is_finite());
            assert_eq!(play.session.as_ref().unwrap().configuration(), Some(config));
            let texts = painted_texts(&out);
            match play.session.as_ref().unwrap().scene() {
                GameScene::Puzzle(p::PuzzleScene::Solo(s)) => {
                    let requested = match config {
                        p::PuzzleConfig::Solo(c) => c.difficulty,
                        p::PuzzleConfig::SoloVariant(c) => c.base.difficulty,
                        _ => panic!(),
                    };
                    assert_eq!(s.certificate.achieved, requested, "{}", preset.name);
                    assert!(
                        texts.iter().any(|(_, t)| t
                            .contains(&format!("Difficulty: {:?}", s.certificate.achieved))),
                        "{}",
                        preset.name
                    )
                }
                GameScene::Puzzle(p::PuzzleScene::Map(s)) => {
                    let p::PuzzleConfig::Map(requested) = config else {
                        panic!()
                    };
                    assert_eq!(
                        s.certificate.achieved, requested.difficulty,
                        "{}",
                        preset.name
                    );
                    assert!(
                        texts.iter().any(|(_, t)| t
                            .contains(&format!("Difficulty: {:?}", s.certificate.achieved))),
                        "{}",
                        preset.name
                    )
                }
                _ => panic!(),
            }
            tested += 1;
        }
        assert_eq!(tested, 15);
    }
    #[test]
    fn advanced_ui_actual_menu_selects_intermediate_and_creates_new_board() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Solo, u64::MAX);
        let size = vec2(1200., 1000.);
        frame(&ctx, &mut w, vec![], size);
        let before = w.games.play.session.as_ref().unwrap().scene();
        let pos = label_position(&ctx, &mut w, "Puzzle options — apply with New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let pos = label_position(&ctx, &mut w, "Choose preset…");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        let pos = label_position(&ctx, &mut w, "3x3 Intermediate");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert_eq!(
            w.games.play.draft,
            Some(advanced_preset("3x3 Intermediate"))
        );
        assert_eq!(w.games.play.session.as_ref().unwrap().scene(), before);
        let pos = label_position(&ctx, &mut w, "New game");
        click(&ctx, &mut w, pos, PointerButton::Primary, size);
        assert_eq!(w.games.play.session.as_ref().unwrap().seed(), 0);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().configuration(),
            Some(advanced_preset("3x3 Intermediate"))
        );
        assert!(!w.games.play.session.as_ref().unwrap().assisted());
    }
    #[test]
    fn advanced_ui_actual_jigsaw_and_killer_cell_hits_keys_solve_buttons() {
        for name in ["9 Jigsaw Basic X", "3x3 Killer"] {
            let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
            assert!(w.games.play.start_config(0, advanced_preset(name)));
            let size = vec2(1200., 1000.);
            frame(&ctx, &mut w, vec![], size);
            frame(&ctx, &mut w, vec![], size);
            let before = w.games.play.session.as_ref().unwrap().scene();
            let GameScene::Puzzle(p::PuzzleScene::Solo(s)) = &before else {
                panic!()
            };
            let cell = s.givens.iter().rposition(|&v| !v).unwrap();
            let r = w.games.play.board.unwrap();
            let unit = r.width() / 9.;
            let pos = r.min + vec2((cell % 9) as f32 + 0.5, (cell / 9) as f32 + 0.5) * unit;
            click(&ctx, &mut w, pos, PointerButton::Primary, size);
            assert_eq!(w.games.play.selected, cell);
            assert_eq!(ctx.memory(|m| m.focused()), w.games.play.board_focus);
            w.games.play.pencil = true;
            frame(
                &ctx,
                &mut w,
                vec![key_event(Key::Num9, Modifiers::default())],
                size,
            );
            let mut expected = GameSession::new_puzzle(0, advanced_preset(name)).unwrap();
            expected
                .input(GameInput::Puzzle(p::PuzzleInput::Pencil { cell, value: 9 }))
                .unwrap();
            assert_eq!(
                w.games.play.session.as_ref().unwrap().scene(),
                expected.scene()
            );
            let pos = label_position(&ctx, &mut w, "Solve…");
            click(&ctx, &mut w, pos, PointerButton::Primary, size);
            assert!(!w.games.play.session.as_ref().unwrap().assisted());
            let pos = label_position(&ctx, &mut w, "Reveal solution");
            click(&ctx, &mut w, pos, PointerButton::Primary, size);
            assert!(w.games.play.session.as_ref().unwrap().assisted());
            let solved = w.games.play.session.as_ref().unwrap().scene();
            let pos = label_position(&ctx, &mut w, "Undo");
            click(&ctx, &mut w, pos, PointerButton::Primary, size);
            assert_eq!(
                w.games.play.session.as_ref().unwrap().scene(),
                expected.scene()
            );
            let pos = label_position(&ctx, &mut w, "Redo");
            click(&ctx, &mut w, pos, PointerButton::Primary, size);
            assert_eq!(w.games.play.session.as_ref().unwrap().scene(), solved);
        }
    }
    fn save_ui_wait(ctx: &Context, w: &mut Workspace) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while w.games.play.files.busy() && Instant::now() < deadline {
            frame(ctx, w, vec![], vec2(1200., 1000.));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !w.games.play.files.busy(),
            "bounded worker did not complete"
        );
    }
    fn save_ui_open_overwrite(ctx: &Context, w: &mut Workspace) {
        save_ui_click(ctx, w, "Save…");
        let deadline = Instant::now() + Duration::from_secs(5);
        while w.games.play.files.prompt.is_none() && Instant::now() < deadline {
            frame(ctx, w, vec![], vec2(1200., 1000.));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.games.play.files.prompt.is_some());
    }
    fn save_ui_click(ctx: &Context, w: &mut Workspace, label: &str) {
        let pos = label_position(ctx, w, label);
        click(ctx, w, pos, PointerButton::Primary, vec2(1200., 1000.));
    }
    #[test]
    fn narrow_game_actions_wrap_and_help_remains_clickable() {
        let (ctx, _root, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 7);
        for scale in [1., 1.5, 2.] {
            let size = vec2(360., 560.);
            let draw = |w: &mut Workspace, events| {
                crate::ui::gui_tests::shell_frame(&ctx, size, scale, events, |ui| {
                    w.games_frame(ui);
                })
            };
            draw(&mut w, vec![]);
            let out = draw(&mut w, vec![]);
            let mut positions = Vec::new();
            for label in ["Save…", "Load…", "New game", "Undo", "Redo", "Help"] {
                let pos = crate::ui::gui_tests::shell_text_position(&out, label);
                assert!(Rect::from_min_size(Pos2::ZERO, size).contains(pos));
                positions.push(pos);
            }
            let pos = *positions.last().unwrap();
            assert!(pos.y > positions[0].y);
            let before = w.games.play.help;
            draw(
                &mut w,
                vec![
                    Event::PointerMoved(pos),
                    crate::ui::gui_tests::shell_pointer(pos, true),
                ],
            );
            draw(
                &mut w,
                vec![crate::ui::gui_tests::shell_pointer(pos, false)],
            );
            assert_ne!(w.games.play.help, before);
        }
    }

    #[test]
    fn save_ui_actual_all_thirteen_controls_and_save_snapshot_during_chooser() {
        use std::sync::{Arc, Mutex, mpsc};
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        for spec in GameRegistry::catalog() {
            w.games.play.start(spec.id, 7);
            w.games.play.files.chooser = Some(Arc::new(|_| None));
            label_position(&ctx, &mut w, "Save…");
            label_position(&ctx, &mut w, "Load…");
            save_ui_click(&ctx, &mut w, "Save…");
            save_ui_wait(&ctx, &mut w);
            assert_eq!(w.games.play.session.as_ref().unwrap().id(), spec.id);
        }
        w.games.play.start(GameId::Ctris, 7);
        let before = w.games.play.session.as_ref().unwrap().clone();
        let path = dir.path().join("saved.termcraft-game");
        let selected = path.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let rx = Arc::new(Mutex::new(rx));
        let gui = std::thread::current().id();
        w.games.play.files.chooser = Some(Arc::new(move |save| {
            assert!(save);
            assert_ne!(std::thread::current().id(), gui);
            rx.lock().unwrap().recv().unwrap();
            Some(selected.clone())
        }));
        save_ui_click(&ctx, &mut w, "Save…");
        assert!(w.games.play.files.busy());
        w.games.play.terminal(t::TerminalInput::Tick);
        assert!(w.games.play.files.dirty());
        tx.send(()).unwrap();
        save_ui_wait(&ctx, &mut w);
        let loaded = GameSession::load_bytes(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(loaded.scene(), before.scene());
        assert!(w.games.play.files.dirty());
        assert!(w.games.play.message.contains("newer"));
    }
    fn save_ui_choice_output(ctx: &Context, w: &mut Workspace, label: &str) -> egui::FullOutput {
        let pos = label_position(ctx, w, label);
        let size = vec2(1200., 1000.);
        frame(
            ctx,
            w,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            size,
        );
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events: vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                }],
                ..Default::default()
            },
            |ui| {
                w.games_frame(ui);
            },
        );
        out.textures_delta.clear();
        out
    }
    fn save_ui_root_close(ctx: &Context, w: &mut Workspace, render: bool) -> egui::FullOutput {
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1000.))),
            ..Default::default()
        };
        raw.viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut out = ctx.run_ui(raw, |ui| {
            eframe::App::logic(w, ui.ctx(), &mut eframe::Frame::_new_kittest());
            if render {
                w.games_frame(ui);
            }
        });
        out.textures_delta.clear();
        out
    }
    #[test]
    fn save_ui_native_root_close_clean_dirty_and_suspended_without_owned_terminal() {
        for suspended in [false, true] {
            let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
            assert!(!w.tabs.iter().any(TerminalTab::running));
            w.games.play.start(GameId::Netwalk, 5);
            w.games.play.puzzle(p::PuzzleInput::Rotate {
                cell: 0,
                clockwise: true,
            });
            w.games.play.suspended = suspended;
            let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
            let out = save_ui_root_close(&ctx, &mut w, false);
            assert!(
                out.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::CancelClose)
            );
            assert!(
                !out.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Close)
            );
            assert!(matches!(w.games.play.files.pending, Some(FileAction::Exit)));
            assert!(w.games.play.is_open());
            assert_eq!(
                w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                before
            );
            frame(&ctx, &mut w, vec![], vec2(1200., 1000.));
            save_ui_click(&ctx, &mut w, "Keep playing");
            assert!(w.games.play.files.pending.is_none());
            assert!(w.games.play.files.dirty());
            assert_eq!(
                w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                before
            );
            let out = save_ui_root_close(&ctx, &mut w, true);
            assert!(
                out.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::CancelClose)
            );
            let out = save_ui_choice_output(&ctx, &mut w, "Discard changes and continue");
            assert!(
                out.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Close)
            );
            assert!(w.games.play.session.is_none());
            assert!(w.allow_session_exit);
        }
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        let out = save_ui_root_close(&ctx, &mut w, true);
        assert!(
            !out.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        assert!(w.games.play.files.pending.is_none());
        assert!(w.games.play.suspended);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
    }
    #[test]
    fn save_ui_native_root_close_keeps_owned_terminal_confirmation_after_game_choice() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let id = crate::ui::terminal_review_tests::raw_terminal(&mut w, "dirty-game-native-close");
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        let out = save_ui_root_close(&ctx, &mut w, true);
        assert!(
            out.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::CancelClose)
        );
        assert!(w.pending_session_close.is_none());
        save_ui_click(&ctx, &mut w, "Keep playing");
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert_eq!(w.tabs[0].id, id);
        assert!(w.tabs[0].running());
        save_ui_root_close(&ctx, &mut w, true);
        let out = save_ui_choice_output(&ctx, &mut w, "Discard changes and continue");
        assert!(
            !out.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Close)
        );
        assert!(w.games.play.session.is_none());
        assert!(w.pending_session_close.is_some());
        assert!(!w.allow_session_exit);
        assert!(w.tabs[0].running());
        w.resolve_session_close(false, &ctx);
        assert!(w.tabs[0].running());
        assert!(!w.allow_session_exit);
        // A retained dirty game also guards an already-pending terminal Close.
        w.request_window_session_close(&ctx);
        assert!(w.pending_session_close.is_some());
        w.games.play.start(GameId::Netwalk, 9);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        w.resolve_session_close(true, &ctx);
        assert!(matches!(w.games.play.files.pending, Some(FileAction::Exit)));
        assert!(!w.allow_session_exit);
        assert!(w.tabs[0].running());
    }
    #[test]
    fn save_ui_adoption_quarantines_old_board_input_and_restores_draft() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Guess, 5);
        let candidate =
            GameSession::new_puzzle(8, p::PuzzleConfig::Net(p::NetConfig::default())).unwrap();
        let expected = candidate.save_bytes().unwrap();
        let config = candidate.configuration();
        let mut out = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 1000.))),
                events: vec![
                    key_event(Key::ArrowRight, Modifiers::default()),
                    Event::Text("old board input".into()),
                ],
                ..Default::default()
            },
            |ui| {
                w.games.play.adopt_file(candidate.clone(), ui.ctx());
                w.games_frame(ui);
            },
        );
        out.textures_delta.clear();
        assert_eq!(w.games.play.selected, 0);
        assert_eq!(w.games.play.draft, config);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            expected
        );
        assert!(
            ctx.input(|i| i.events.is_empty() && i.keys_down.is_empty() && !i.pointer.any_down())
        );
    }
    #[test]
    fn actual_game_escape_keeps_dirty_game_and_does_not_play_the_queued_move() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        w.games.play.request_file_action(FileAction::Close, &ctx);
        assert!(w.games.play.files.pending.is_some());
        frame(&ctx, &mut w, vec![], vec2(1200., 1000.));
        label_position(&ctx, &mut w, "Keep playing");
        frame(
            &ctx,
            &mut w,
            vec![
                Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::Text("x".into()),
            ],
            vec2(1200., 1000.),
        );
        assert!(w.games.play.files.pending.is_none());
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert!(w.games.play.files.dirty());
    }
    #[test]
    fn save_ui_actual_discard_resets_creates_and_closes() {
        let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        for label in ["Reset", "New game", "Close (Esc)"] {
            w.games.play.puzzle(p::PuzzleInput::Rotate {
                cell: 0,
                clockwise: true,
            });
            assert!(w.games.play.files.dirty());
            save_ui_click(&ctx, &mut w, label);
            assert!(w.games.play.files.pending.is_some());
            save_ui_click(&ctx, &mut w, "Discard changes and continue");
            assert!(!w.games.play.files.dirty());
            if label == "Reset" {
                assert!(!w.games.play.session.as_ref().unwrap().can_undo());
            }
            if label == "New game" {
                assert_eq!(w.games.play.session.as_ref().unwrap().seed(), 6);
            }
        }
        assert!(w.games.play.session.is_none());
        assert!(!w.games.play.is_open());
    }
    #[test]
    fn save_ui_actual_overwrite_requires_explicit_consent() {
        use std::sync::Arc;
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        let path = dir.path().join("existing");
        std::fs::write(&path, b"keep original").unwrap();
        let chosen = path.clone();
        w.games.play.files.chooser = Some(Arc::new(move |save| {
            assert!(save);
            Some(chosen.clone())
        }));
        save_ui_open_overwrite(&ctx, &mut w);
        save_ui_click(&ctx, &mut w, "Keep existing file");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(std::fs::read(&path).unwrap(), b"keep original");
        assert!(w.games.play.files.dirty());
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        save_ui_open_overwrite(&ctx, &mut w);
        frame(
            &ctx,
            &mut w,
            vec![key_event(Key::Escape, Modifiers::NONE)],
            vec2(1200., 1000.),
        );
        save_ui_wait(&ctx, &mut w);
        assert_eq!(std::fs::read(&path).unwrap(), b"keep original");
        assert!(w.games.play.files.dirty());

        save_ui_open_overwrite(&ctx, &mut w);
        save_ui_root_close(&ctx, &mut w, false);
        save_ui_wait(&ctx, &mut w);
        assert!(matches!(w.games.play.files.pending, Some(FileAction::Exit)));
        assert!(w.games.play.files.prompt.is_none());
        save_ui_click(&ctx, &mut w, "Keep playing");
        assert_eq!(std::fs::read(&path).unwrap(), b"keep original");
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert!(w.games.play.files.dirty());

        save_ui_open_overwrite(&ctx, &mut w);
        save_ui_click(&ctx, &mut w, "Replace file");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!w.games.play.files.dirty());
    }
    #[test]
    fn save_ui_actual_load_failure_guards_and_transient_reset() {
        use std::sync::Arc;
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        for label in ["New game", "Reset", "Close (Esc)", "Load…"] {
            save_ui_click(&ctx, &mut w, label);
            assert!(w.games.play.files.pending.is_some());
            assert_eq!(
                w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                before
            );
            save_ui_click(&ctx, &mut w, "Keep playing");
            assert!(w.games.play.files.pending.is_none());
        }
        let path = dir.path().join("invalid");
        std::fs::write(&path, b"bad game file").unwrap();
        let chosen = path.clone();
        w.games.play.files.chooser = Some(Arc::new(move |save| {
            assert!(!save);
            Some(chosen.clone())
        }));
        save_ui_click(&ctx, &mut w, "Load…");
        save_ui_click(&ctx, &mut w, "Discard changes and continue");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"bad game file");
        assert!(w.games.play.message.starts_with("Game file:"));
        let candidate = GameSession::new_solitaire(10, t::SolitaireVariant::Klondike, 9);
        std::fs::write(&path, candidate.save_bytes().unwrap()).unwrap();
        w.games.play.dragging = Some(7);
        w.games.play.source = Some(t::CardPile::Reserve(0));
        w.games.play.tick = Some(Instant::now());
        save_ui_click(&ctx, &mut w, "Load…");
        save_ui_click(&ctx, &mut w, "Discard changes and continue");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().scene(),
            candidate.scene()
        );
        assert_eq!((w.games.play.variant, w.games.play.passes), (1, 9));
        assert!(w.games.play.dragging.is_none());
        assert!(w.games.play.source.is_none());
        assert!(w.games.play.tick.is_none());
        assert!(!w.games.play.files.dirty());
    }
    #[test]
    fn save_ui_actual_load_during_ticks_reconfirms_and_new_generation_ignores_candidate() {
        use std::sync::{Arc, Mutex, mpsc};
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        let candidate = GameRegistry::start(GameId::Guess, 13);
        let path = dir.path().join("candidate");
        std::fs::write(&path, candidate.save_bytes().unwrap()).unwrap();
        w.games.play.start(GameId::Ctris, 7);
        let (tx, rx) = mpsc::sync_channel(1);
        let rx = Arc::new(Mutex::new(rx));
        let chosen = path.clone();
        w.games.play.files.chooser = Some(Arc::new(move |_| {
            rx.lock().unwrap().recv().unwrap();
            Some(chosen.clone())
        }));
        save_ui_click(&ctx, &mut w, "Load…");
        w.games.play.terminal(t::TerminalInput::Tick);
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        tx.send(()).unwrap();
        save_ui_wait(&ctx, &mut w);
        assert!(matches!(
            w.games.play.files.pending,
            Some(FileAction::Adopt(_))
        ));
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        save_ui_click(&ctx, &mut w, "Keep playing");
        let (tx, rx) = mpsc::sync_channel(1);
        let rx = Arc::new(Mutex::new(rx));
        w.games.play.files.chooser = Some(Arc::new(move |_| {
            rx.lock().unwrap().recv().unwrap();
            Some(path.clone())
        }));
        save_ui_click(&ctx, &mut w, "Load…");
        save_ui_click(&ctx, &mut w, "Discard changes and continue");
        w.games.play.start(GameId::Termwalk, 42);
        let current = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        tx.send(()).unwrap();
        save_ui_wait(&ctx, &mut w);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            current
        );
    }

    #[test]
    fn review_save_ui_replacement_during_visible_overwrite_consent_is_preserved() {
        use std::sync::Arc;
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        let path = dir.path().join("existing");
        std::fs::write(&path, b"original file user was asked about").unwrap();
        let chosen = path.clone();
        w.games.play.files.chooser = Some(Arc::new(move |_| Some(chosen.clone())));
        save_ui_open_overwrite(&ctx, &mut w);
        {
            let path = &path;
            // While confirmation is visible, another producer replaces the path.
            // Rename keeps both old and replacement inodes alive, eliminating inode reuse.
            std::fs::rename(path, path.with_extension("old")).unwrap();
            std::fs::write(path, b"replacement not covered by consent").unwrap();
        }
        save_ui_click(&ctx, &mut w, "Replace file");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert_eq!(
            std::fs::read(path.with_extension("old")).unwrap(),
            b"original file user was asked about"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"replacement not covered by consent",
            "consent to replace the old inode must not authorize replacing a new file"
        );
        assert!(w.games.play.files.dirty());
    }

    #[test]
    fn fix1_save_ui_in_place_edit_during_visible_overwrite_consent_is_preserved() {
        use std::os::unix::fs::MetadataExt;
        use std::sync::Arc;
        let (ctx, dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
        w.games.play.start(GameId::Netwalk, 5);
        w.games.play.puzzle(p::PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        });
        let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
        let path = dir.path().join("existing");
        const OLD: &[u8] = b"old contents, same size";
        const NEW: &[u8] = b"new contents, same size";
        std::fs::write(&path, OLD).unwrap();
        let chosen = path.clone();
        w.games.play.files.chooser = Some(Arc::new(move |_| Some(chosen.clone())));
        save_ui_open_overwrite(&ctx, &mut w);
        {
            let path = &path;
            let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            let original = file.metadata().unwrap();
            std::fs::write(path, NEW).unwrap();
            // Keep inode, length and mtime identical: bounded content comparison
            // must catch this edit rather than relying on timestamp resolution.
            file.set_times(std::fs::FileTimes::new().set_modified(original.modified().unwrap()))
                .unwrap();
            let changed = file.metadata().unwrap();
            assert_eq!(
                (
                    original.dev(),
                    original.ino(),
                    original.len(),
                    original.modified().unwrap()
                ),
                (
                    changed.dev(),
                    changed.ino(),
                    changed.len(),
                    changed.modified().unwrap()
                )
            );
        }
        save_ui_click(&ctx, &mut w, "Replace file");
        save_ui_wait(&ctx, &mut w);
        assert_eq!(std::fs::read(&path).unwrap(), NEW);
        assert_eq!(
            w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
            before
        );
        assert!(w.games.play.files.dirty());
        assert!(w.games.play.message.contains("target changed"));
    }

    #[test]
    fn fix2_actual_new_failure_advances_attempt_and_cancel_preserves_game() {
        for (id, desired) in [
            (GameId::Solo, advanced_preset("3x3 Advanced")),
            (
                GameId::Map,
                p::PuzzleConfig::Map(p::MapConfig {
                    preset: p::MapPreset::Default,
                    regions: 30,
                    difficulty: p::MapDifficulty::Unreasonable,
                }),
            ),
        ] {
            let first_valid = (2..=16)
                .find(|&seed| GameSession::new_puzzle(seed, desired).is_ok())
                .expect("bounded independent graded sample");
            assert!(
                first_valid > 2,
                "native failed seed2 fixture must exercise failure: {desired:?}"
            );
            eprintln!("{desired:?}: independent first valid seed after1 is {first_valid}");
            let (ctx, _dir, mut w) = crate::ui::terminal_review_tests::owned_workspace();
            w.games.play.start(id, 1);
            w.games.play.session.as_mut().unwrap().solve().unwrap();
            w.games.play.files.changed();
            w.games.play.selected = 7;
            w.games.play.draft = Some(desired);
            let before = w.games.play.session.as_ref().unwrap().save_bytes().unwrap();
            let file_state = (
                w.games.play.files.generation,
                w.games.play.files.revision,
                w.games.play.files.clean,
            );
            save_ui_click(&ctx, &mut w, "New game");
            save_ui_click(&ctx, &mut w, "Keep playing");
            assert_eq!(
                w.games.play.next_seed, 2,
                "cancel must not consume an attempt"
            );
            assert_eq!(
                w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                before
            );
            for seed in 2..=first_valid {
                save_ui_click(&ctx, &mut w, "New game");
                save_ui_click(&ctx, &mut w, "Discard changes and continue");
                assert_eq!(w.games.play.next_seed, seed.wrapping_add(1));
                if seed < first_valid {
                    assert_eq!(
                        w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                        before
                    );
                    assert_eq!(
                        (
                            w.games.play.files.generation,
                            w.games.play.files.revision,
                            w.games.play.files.clean
                        ),
                        file_state
                    );
                    assert_eq!(w.games.play.selected, 7);
                    assert_eq!(w.games.play.draft, Some(desired));
                    assert!(w.games.play.message.contains("Your current game is kept"));
                    assert!(w.games.play.files.dirty());
                } else {
                    let expected = GameSession::new_puzzle(seed, desired).unwrap();
                    assert_eq!(
                        w.games.play.session.as_ref().unwrap().save_bytes().unwrap(),
                        expected.save_bytes().unwrap()
                    );
                    assert!(!w.games.play.files.dirty());
                    assert!(!w.games.play.session.as_ref().unwrap().assisted());
                }
            }
        }
    }
    #[test]
    fn fix2_attempt_cursor_wraps_and_reinitializes_for_all_start_reset_load_paths() {
        let ctx = Context::default();
        let mut play = PlayState::default();
        play.perform_file_action(FileAction::New, &ctx);
        assert_eq!(play.next_seed, 1);
        for spec in GameRegistry::catalog() {
            play.start(spec.id, u64::MAX);
            assert_eq!(play.next_seed, 0, "{:?}", spec.id);
            play.perform_file_action(FileAction::New, &ctx);
            assert_eq!(play.next_seed, 1, "{:?}", spec.id);
            play.next_seed = 99;
            play.perform_file_action(FileAction::Reset, &ctx);
            assert_eq!(
                play.next_seed,
                play.session.as_ref().unwrap().seed().wrapping_add(1)
            );
            let loaded = play.session.as_ref().unwrap().clone();
            let expected = loaded.seed().wrapping_add(1);
            play.next_seed = 123;
            play.adopt_file(loaded, &ctx);
            assert_eq!(play.next_seed, expected);
        }
        play.start(GameId::Solo, u64::MAX - 1);
        play.draft = Some(p::PuzzleConfig::SoloVariant(p::SoloVariantConfig {
            base: p::SoloConfig::default(),
            jigsaw: true,
            killer: true,
        }));
        let before = play.session.as_ref().unwrap().save_bytes().unwrap();
        for next in [0, 1] {
            play.perform_file_action(FileAction::New, &ctx);
            assert_eq!(play.next_seed, next);
            assert_eq!(play.session.as_ref().unwrap().save_bytes().unwrap(), before);
        }
        play.perform_file_action(FileAction::Reset, &ctx);
        assert_eq!(play.next_seed, u64::MAX);
        assert!(play.start_config(12, p::PuzzleConfig::Guess(p::GuessConfig::default())));
        assert_eq!(play.next_seed, 13);
    }
    #[test]
    fn fix2_failed_confirmed_new_keeps_queued_input_and_active_settings() {
        let ctx = Context::default();
        let mut play = PlayState::default();
        play.start(GameId::Solo, 1);
        play.session.as_mut().unwrap().solve().unwrap();
        play.files.changed();
        let invalid = p::PuzzleConfig::SoloVariant(p::SoloVariantConfig {
            base: p::SoloConfig::default(),
            jigsaw: true,
            killer: true,
        });
        play.draft = Some(invalid);
        play.selected = 7;
        play.colour = 4;
        play.pencil = true;
        let before = play.session.as_ref().unwrap().save_bytes().unwrap();
        let files = (play.files.generation, play.files.revision, play.files.clean);
        let mut out = ctx.run_ui(
            RawInput {
                events: vec![
                    key_event(Key::ArrowRight, Modifiers::CTRL),
                    Event::Paste("queued input".into()),
                ],
                ..Default::default()
            },
            |ui| {
                let queued = ui
                    .ctx()
                    .input(|i| (i.events.clone(), i.keys_down.clone(), i.modifiers));
                play.perform_file_action(FileAction::New, ui.ctx());
                assert_eq!(
                    ui.ctx()
                        .input(|i| (i.events.clone(), i.keys_down.clone(), i.modifiers)),
                    queued
                );
            },
        );
        out.textures_delta.clear();
        assert_eq!(play.session.as_ref().unwrap().save_bytes().unwrap(), before);
        assert_eq!(
            (play.files.generation, play.files.revision, play.files.clean),
            files
        );
        assert_eq!(
            (play.draft, play.selected, play.colour, play.pencil),
            (Some(invalid), 7, 4, true)
        );
        assert_eq!(play.next_seed, 3);
        let expected_error = GameSession::new_puzzle(2, invalid).err().unwrap();
        assert_eq!(play.message, player_error(expected_error));
    }
}
