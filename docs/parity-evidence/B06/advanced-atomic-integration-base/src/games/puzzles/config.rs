use super::*;

/// Local, bounded configurations. These are not upstream encoded parameters or IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleConfig {
    Net(NetConfig),
    Mines(MinesConfig),
    Solo(SoloConfig),
    Guess(GuessConfig),
    Map(MapConfig),
    Untangle { points: usize },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzlePreset {
    pub name: String,
    /// None means a recovered stock option that is not yet implemented.
    pub configuration: Option<PuzzleConfig>,
    pub note: &'static str,
}
impl PuzzleConfig {
    pub fn kind(self) -> PuzzleKind {
        match self {
            Self::Net(_) => PuzzleKind::Net,
            Self::Mines(_) => PuzzleKind::Mines,
            Self::Solo(_) => PuzzleKind::Solo,
            Self::Guess(_) => PuzzleKind::Guess,
            Self::Map(_) => PuzzleKind::Map,
            Self::Untangle { .. } => PuzzleKind::Untangle,
        }
    }
    pub fn validate(self) -> Result<(), &'static str> {
        match self {
            Self::Net(c) => c.validate(),
            Self::Mines(c) => c.validate(),
            Self::Solo(c) => c.validate(),
            Self::Guess(c) => c.validate(),
            Self::Map(c) => c.validate(),
            Self::Untangle { points } if !(4..=25).contains(&points) => {
                Err("Untangle supports 4 through 25 points")
            }
            Self::Untangle { .. } => Ok(()),
        }
    }
    pub fn help(self) -> String {
        if let Err(error) = self.validate() {
            return format!("Invalid puzzle configuration: {error}");
        }
        match self {
            Self::Net(c) => format!(
                "{}×{} Net. {}. Connect every tile as one tree without loops. Barriers block connections. Barrier probability {}%; {}. {}",
                c.width,
                c.height,
                if c.wrapping {
                    "Opposite edges wrap"
                } else {
                    "Edges do not wrap"
                },
                c.barrier_percent,
                if c.unique {
                    "unique solution required; bounded generation can return an error"
                } else {
                    "multiple solutions allowed"
                },
                PuzzleKind::Net.help()
            ),
            Self::Mines(c) => format!(
                "{}×{} Mines, {} mines. {}. First reveal and its neighbours are safe. {}",
                c.width,
                c.height,
                c.mines,
                if c.solvable {
                    "Ensure-solvable uses a complete deduction replay; bounded generation may fail explicitly"
                } else {
                    "Guessing may be necessary"
                },
                PuzzleKind::Mines.help()
            ),
            Self::Solo(c) => format!(
                "{}×{} Solo with {}×{} boxes. Digits 1–{} occur once per row, column and box{}. Requested {:?}; read the scene certificate for achieved logical difficulty and disclosed easier fallback. Clue symmetry {:?}. 0 erases; starting digits are fixed.",
                c.block_width * c.block_height,
                c.block_width * c.block_height,
                c.block_width,
                c.block_height,
                c.block_width * c.block_height,
                if c.diagonal {
                    ", and on each long diagonal"
                } else {
                    ""
                },
                c.difficulty,
                c.symmetry
            ),
            Self::Guess(c) => format!(
                "Find {} hidden pegs with {} colours within {} guesses. Black = correct position; white = correct colour elsewhere. {}. {}. Hold transfers a peg to the next row. Solve gives up and reveals the code.",
                c.pegs,
                c.colours,
                c.max_guesses,
                if c.allow_multiple {
                    "Repeated colours allowed"
                } else {
                    "Repeated colours forbidden"
                },
                if c.allow_blank {
                    "Blank guesses allowed, but at least one peg is required; the hidden code never contains blanks"
                } else {
                    "Fill every peg before marking"
                }
            ),
            Self::Map(c) => {
                let (w, h) = c.dimensions();
                format!(
                    "{}×{} Map, {} regions, {:?}. {} Only edge-sharing regions are adjacent. Pencil marks require empty editable regions. Bounded generation may fail explicitly.",
                    w,
                    h,
                    c.regions,
                    c.difficulty,
                    PuzzleKind::Map.help()
                )
            }
            Self::Untangle { points } => format!(
                "{} points. {} Independent generator currently uses outerplanar triangulations.",
                points,
                PuzzleKind::Untangle.help()
            ),
        }
    }
}
impl PuzzleKind {
    pub fn default_configuration(self) -> PuzzleConfig {
        match self {
            Self::Net => PuzzleConfig::Net(NetConfig::default()),
            Self::Mines => PuzzleConfig::Mines(MinesConfig::default()),
            Self::Solo => PuzzleConfig::Solo(SoloConfig::default()),
            Self::Guess => PuzzleConfig::Guess(GuessConfig::default()),
            Self::Map => PuzzleConfig::Map(MapConfig::default()),
            Self::Untangle => PuzzleConfig::Untangle { points: 10 },
        }
    }
    /// Exact f306b9d stock menu definitions, including both conditional Map
    /// orientations and non-SMALL_SCREEN entries. Unsupported grades are disabled.
    pub fn presets(self) -> Vec<PuzzlePreset> {
        let mut out = Vec::new();
        let mut add = |name: String, configuration: Option<PuzzleConfig>, note| {
            out.push(PuzzlePreset {
                name,
                configuration,
                note,
            })
        };
        match self {
            Self::Net => {
                for wrapping in [false, true] {
                    for (width, height) in [(5, 5), (7, 7), (9, 9), (11, 11), (13, 11)] {
                        add(
                            format!(
                                "{width}x{height}{}",
                                if wrapping { " wrapping" } else { "" }
                            ),
                            Some(PuzzleConfig::Net(NetConfig {
                                width,
                                height,
                                wrapping,
                                ..NetConfig::default()
                            })),
                            "Exact stock settings; independent generator, bounded uniqueness search may return an error.",
                        );
                    }
                }
            }
            Self::Mines => {
                for (width, height, mines) in [
                    (9, 9, 10),
                    (9, 9, 35),
                    (16, 16, 40),
                    (16, 16, 99),
                    (30, 16, 99),
                    (30, 16, 170),
                ] {
                    add(
                        format!("{width}x{height}, {mines} mines"),
                        Some(PuzzleConfig::Mines(MinesConfig {
                            width,
                            height,
                            mines,
                            solvable: true,
                        })),
                        "No-guess deduction replay is mandatory; bounded first-reveal generation may return an error.",
                    );
                }
            }
            Self::Guess => {
                for (name, colours, pegs, max_guesses) in
                    [("Standard", 6, 4, 10), ("Super", 8, 5, 12)]
                {
                    add(
                        name.into(),
                        Some(PuzzleConfig::Guess(GuessConfig {
                            colours,
                            pegs,
                            max_guesses,
                            ..GuessConfig::default()
                        })),
                        "Exact stock rules; independent hidden-code generation.",
                    );
                }
            }
            Self::Untangle => {
                for points in [6, 10, 15, 20, 25] {
                    add(
                        format!("{points} points"),
                        Some(PuzzleConfig::Untangle { points }),
                        "Independent outerplanar subset, not the complete upstream planar graph distribution.",
                    );
                }
            }
            Self::Solo => {
                // Historical c×r titles use physical box width r, height c
                // (solo.c:3611), unlike our explicit width/height field names.
                for (name, bw, bh, diagonal, difficulty) in [
                    ("2x2 Trivial", 2, 2, false, SoloDifficulty::Trivial),
                    ("2x3 Basic", 3, 2, false, SoloDifficulty::Basic),
                    ("3x3 Trivial", 3, 3, false, SoloDifficulty::Trivial),
                    ("3x3 Basic", 3, 3, false, SoloDifficulty::Basic),
                    ("3x3 Basic X", 3, 3, true, SoloDifficulty::Basic),
                ] {
                    add(
                        name.into(),
                        Some(PuzzleConfig::Solo(SoloConfig {
                            block_width: bw,
                            block_height: bh,
                            diagonal,
                            difficulty,
                            ..SoloConfig::default()
                        })),
                        "Source-rule logical certificate reports achieved grade; a Basic request may disclose an easier Trivial fallback.",
                    );
                }
                for name in [
                    "3x3 Intermediate",
                    "3x3 Advanced",
                    "3x3 Advanced X",
                    "3x3 Extreme",
                    "3x3 Unreasonable",
                    "3x3 Killer",
                    "9 Jigsaw Basic",
                    "9 Jigsaw Basic X",
                    "9 Jigsaw Advanced",
                ] {
                    add(
                        name.into(),
                        None,
                        "Pending source-defined advanced deduction, cage or irregular-region support; cannot select this variant.",
                    );
                }
                for (name, bw, bh) in [("3x4 Basic", 4, 3), ("4x4 Basic", 4, 4)] {
                    add(
                        name.into(),
                        Some(PuzzleConfig::Solo(SoloConfig {
                            block_width: bw,
                            block_height: bh,
                            difficulty: SoloDifficulty::Basic,
                            ..SoloConfig::default()
                        })),
                        "Non-SLOW_SYSTEM stock entry; certificate discloses any easier Trivial fallback. Digits 10–16 need UI support.",
                    );
                }
            }
            Self::Map => {
                for preset in [MapPreset::Default, MapPreset::Portrait] {
                    for (regions, difficulty, label) in [
                        (30, Some(MapDifficulty::Easy), "Easy"),
                        (30, Some(MapDifficulty::Normal), "Normal"),
                        (30, None, "Hard"),
                        (30, None, "Unreasonable"),
                        (75, Some(MapDifficulty::Normal), "Normal"),
                        (75, None, "Hard"),
                    ] {
                        let cfg = MapConfig {
                            preset,
                            regions,
                            difficulty: difficulty.unwrap_or(MapDifficulty::Normal),
                        };
                        let (w, h) = cfg.dimensions();
                        add(
                            format!("{w}x{h}, {regions} regions, {label}"),
                            difficulty.map(|_| PuzzleConfig::Map(cfg)),
                            if difficulty.is_some() {
                                "Complete source Easy/Normal replay; large-board bounded generation can return an error."
                            } else {
                                "Pending historical Hard/Unreasonable deduction support; cannot select this variant."
                            },
                        );
                    }
                }
            }
        }
        out
    }
}
