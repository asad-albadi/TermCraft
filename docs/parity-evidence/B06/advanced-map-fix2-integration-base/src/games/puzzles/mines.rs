use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinesConfig {
    pub width: usize,
    pub height: usize,
    pub mines: usize,
    pub solvable: bool,
}
impl Default for MinesConfig {
    fn default() -> Self {
        Self {
            width: 9,
            height: 9,
            mines: 10,
            solvable: true,
        }
    }
}
impl MinesConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if !(3..=30).contains(&self.width) || !(3..=16).contains(&self.height) {
            return Err("Mines supports width 3–30 and height 3–16");
        }
        if self.mines == 0 || self.mines > self.width * self.height - 9 {
            return Err("Mine count must be between 1 and board area minus 9");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinesGeneration {
    Unplaced,
    Randomized,
    CertifiedFallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineCell {
    Covered,
    Flagged,
    Open(u8),
    Exploded,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinesScene {
    /// Hidden until the user explicitly invokes Solve, including after death.
    pub solution: Option<Vec<bool>>,
    pub generation: MinesGeneration,
    pub solvable: bool,
    pub width: usize,
    pub height: usize,
    pub mine_count: usize,
    pub cells: Vec<MineCell>,
    pub flags: usize,
    pub deaths: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mines {
    config: MinesConfig,
    generation: MinesGeneration,
    solution_revealed: bool,
    seed: u64,
    mines: Vec<bool>,
    revealed: Vec<bool>,
    flagged: Vec<bool>,
    placed: bool,
    exploded: Option<usize>,
}
#[cfg(test)]
fn number(mines: &[bool], i: usize) -> u8 {
    number_config(mines, i, MinesConfig::default())
}
fn number_config(mines: &[bool], i: usize, c: MinesConfig) -> u8 {
    neighbours(i, c.width, c.height, true)
        .iter()
        .filter(|&&j| mines[j])
        .count() as u8
}
fn flood(mines: &[bool], revealed: &mut [bool], flagged: &[bool], start: usize, c: MinesConfig) {
    let mut q = vec![start];
    while let Some(i) = q.pop() {
        if revealed[i] || flagged[i] || mines[i] {
            continue;
        }
        revealed[i] = true;
        if number_config(mines, i, c) == 0 {
            q.extend(neighbours(i, c.width, c.height, true));
        }
    }
}
/// Candidate acceptance uses deductions only: zero/all-mine sets and subset differences.
#[cfg(test)]
fn logical(mines: &[bool], start: usize) -> bool {
    logical_config(mines, start, MinesConfig::default(), &mut 2_000_000)
}
fn logical_config(mines: &[bool], start: usize, c: MinesConfig, budget: &mut usize) -> bool {
    let cells = c.width * c.height;
    let mut revealed = vec![false; cells];
    let mut flagged = vec![false; cells];
    flood(mines, &mut revealed, &flagged, start, c);
    for _ in 0..cells {
        if revealed.iter().filter(|&&v| v).count() == cells - c.mines {
            return true;
        }
        let mut constraints: Vec<(Vec<usize>, usize)> = Vec::new();
        for i in 0..cells {
            if revealed[i] {
                let around = neighbours(i, c.width, c.height, true);
                let known = around.iter().filter(|&&j| flagged[j]).count();
                let unknown: Vec<_> = around
                    .into_iter()
                    .filter(|&j| !revealed[j] && !flagged[j])
                    .collect();
                let n = number_config(mines, i, c) as usize;
                if n < known {
                    return false;
                }
                if !unknown.is_empty() {
                    constraints.push((unknown, n - known));
                }
            }
        }
        let unknown: Vec<_> = (0..cells)
            .filter(|&i| !revealed[i] && !flagged[i])
            .collect();
        constraints.push((unknown, c.mines - flagged.iter().filter(|&&v| v).count()));
        let base = constraints.clone();
        for (a, na) in &base {
            for (b, nb) in &base {
                if *budget == 0 {
                    return false;
                }
                *budget -= 1;
                if a.len() < b.len() && na <= nb && a.iter().all(|i| b.contains(i)) {
                    let difference: Vec<_> = b.iter().copied().filter(|i| !a.contains(i)).collect();
                    let n = nb - na;
                    if n == 0 || n == difference.len() {
                        constraints.push((difference, n));
                    }
                }
            }
        }
        let mut progress = false;
        for (cells, n) in constraints {
            if n == 0 {
                for i in cells {
                    if !revealed[i] {
                        if mines[i] {
                            return false;
                        }
                        flood(mines, &mut revealed, &flagged, i, c);
                        progress = true;
                    }
                }
            } else if n == cells.len() {
                for i in cells {
                    if !flagged[i] {
                        if !mines[i] {
                            return false;
                        }
                        flagged[i] = true;
                        progress = true;
                    }
                }
            }
        }
        if !progress {
            return false;
        }
    }
    false
}
impl Mines {
    pub(super) fn solve(&mut self) -> Result<(), &'static str> {
        if !self.placed {
            return Err("Game has not been started yet");
        }
        self.solution_revealed = true;
        if self.exploded.is_some() {
            return Ok(());
        }
        for i in 0..self.mines.len() {
            self.flagged[i] = self.mines[i];
            self.revealed[i] = !self.mines[i];
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn mine_for_test(&self) -> usize {
        self.mines.iter().position(|&x| x).unwrap()
    }

    pub fn new(seed: u64) -> Self {
        Self::new_config(seed, MinesConfig::default()).expect("valid default")
    }
    pub fn new_config(seed: u64, config: MinesConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let cells = config.width * config.height;
        Ok(Self {
            config,
            generation: MinesGeneration::Unplaced,
            solution_revealed: false,
            seed,
            mines: vec![false; cells],
            revealed: vec![false; cells],
            flagged: vec![false; cells],
            placed: false,
            exploded: None,
        })
    }
    fn place(&mut self, start: usize) -> Result<(), &'static str> {
        let c = self.config;
        let cells = c.width * c.height;
        let mut rng = Rng(self.seed ^ (start as u64).wrapping_mul(0x517cc1b727220a95));
        let mut excluded = neighbours(start, c.width, c.height, true);
        excluded.push(start);
        let mut candidates: Vec<_> = (0..cells).filter(|i| !excluded.contains(i)).collect();
        let mut budget = 2_000_000;
        for _ in 0..64 {
            rng.shuffle(&mut candidates);
            self.mines.fill(false);
            for &i in &candidates[..c.mines] {
                self.mines[i] = true;
            }
            if !c.solvable || logical_config(&self.mines, start, c, &mut budget) {
                self.placed = true;
                self.generation = MinesGeneration::Randomized;
                return Ok(());
            }
            if budget == 0 {
                break;
            }
        }
        // Structured local alternatives are accepted by exactly the same proof,
        // never labelled no-guess merely because mines were placed in a block.
        for top in [true, false] {
            for left in [true, false] {
                for transpose in [false, true] {
                    let mut order: Vec<_> = (0..cells).filter(|i| !excluded.contains(i)).collect();
                    order.sort_by_key(|&i| {
                        let (x, y) = (i % c.width, i / c.width);
                        let (x, y) = (
                            if left { x } else { c.width - 1 - x },
                            if top { y } else { c.height - 1 - y },
                        );
                        if transpose {
                            x * c.height + y
                        } else {
                            y * c.width + x
                        }
                    });
                    self.mines.fill(false);
                    for &i in &order[..c.mines] {
                        self.mines[i] = true;
                    }
                    let mut fallback_budget = 500_000;
                    if logical_config(&self.mines, start, c, &mut fallback_budget) {
                        self.placed = true;
                        self.generation = MinesGeneration::CertifiedFallback;
                        return Ok(());
                    }
                }
            }
        }
        Err(
            "No-guess Mines generation budget exhausted; try another seed or disable ensure-solvable",
        )
    }
    pub fn outcome(&self) -> Outcome {
        if self.exploded.is_some() {
            Outcome::Lost
        } else if self.placed
            && self.revealed.iter().filter(|&&v| v).count() == self.mines.len() - self.config.mines
        {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self, deaths: u32) -> MinesScene {
        MinesScene {
            solution: if self.solution_revealed {
                Some(self.mines.clone())
            } else {
                None
            },
            width: self.config.width,
            height: self.config.height,
            mine_count: self.config.mines,
            generation: self.generation,
            solvable: self.config.solvable,
            cells: (0..self.mines.len())
                .map(|i| {
                    if self.exploded == Some(i) {
                        MineCell::Exploded
                    } else if self.revealed[i] {
                        MineCell::Open(number_config(&self.mines, i, self.config))
                    } else if self.flagged[i] {
                        MineCell::Flagged
                    } else {
                        MineCell::Covered
                    }
                })
                .collect(),
            flags: self.flagged.iter().filter(|&&v| v).count(),
            deaths,
        }
    }
    fn reveal(&mut self, i: usize) {
        if self.flagged[i] || self.revealed[i] || self.exploded.is_some() {
            return;
        }
        if self.mines[i] {
            self.exploded = Some(i);
        } else {
            flood(
                &self.mines,
                &mut self.revealed,
                &self.flagged,
                i,
                self.config,
            );
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        let (cell, action) = match input {
            PuzzleInput::Reveal { cell } => (cell, 0),
            PuzzleInput::Flag { cell } => (cell, 1),
            PuzzleInput::Chord { cell } => (cell, 2),
            _ => return Err("Unsupported Mines input"),
        };
        if cell >= self.mines.len() {
            return Err("Square outside board");
        }
        if action == 1 {
            if !self.revealed[cell] {
                self.flagged[cell] = !self.flagged[cell];
            }
            return Ok(());
        }
        if self.flagged[cell] {
            return Ok(());
        }
        if action == 2 && !self.revealed[cell] {
            return Ok(());
        }
        if !self.placed {
            self.place(cell)?;
        }
        if self.revealed[cell] {
            let around = neighbours(cell, self.config.width, self.config.height, true);
            if around.iter().filter(|&&j| self.flagged[j]).count()
                == number_config(&self.mines, cell, self.config) as usize
            {
                for j in around {
                    self.reveal(j);
                }
            }
        } else {
            self.reveal(cell);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_reveal_safe_and_deducible_all_corners() {
        for seed in 0..20 {
            for start in [0, 8, 40, 72, 80] {
                let mut m = Mines::new(seed);
                m.apply(PuzzleInput::Reveal { cell: start }).unwrap();
                assert_eq!(m.mines.iter().filter(|&&v| v).count(), 10);
                assert_eq!(number(&m.mines, start), 0);
                assert!(logical(&m.mines, start));
                assert_ne!(m.outcome(), Outcome::Lost);
                for i in 0..81 {
                    if !m.mines[i] {
                        m.apply(PuzzleInput::Reveal { cell: i }).unwrap();
                    }
                }
                assert_eq!(m.outcome(), Outcome::Won);
            }
        }
    }
    #[test]
    fn flags_chords_loss_and_hidden_mines() {
        let mut m = Mines::new(5);
        m.apply(PuzzleInput::Flag { cell: 40 }).unwrap();
        m.apply(PuzzleInput::Reveal { cell: 40 }).unwrap();
        assert!(!m.placed);
        m.apply(PuzzleInput::Flag { cell: 40 }).unwrap();
        m.apply(PuzzleInput::Reveal { cell: 40 }).unwrap();
        let mine = m.mines.iter().position(|&v| v).unwrap();
        m.apply(PuzzleInput::Reveal { cell: mine }).unwrap();
        assert_eq!(m.outcome(), Outcome::Lost);
        assert_eq!(
            m.scene(1)
                .cells
                .iter()
                .filter(|&&c| c == MineCell::Exploded)
                .count(),
            1
        );
        assert!(m.scene(1).cells.contains(&MineCell::Covered));
        let mut m = Mines::new(88);
        m.apply(PuzzleInput::Reveal { cell: 40 }).unwrap();
        for i in 0..81 {
            if m.mines[i] {
                m.apply(PuzzleInput::Flag { cell: i }).unwrap();
            }
        }
        for i in 0..81 {
            if m.revealed[i] {
                m.apply(PuzzleInput::Chord { cell: i }).unwrap();
            }
        }
        assert_eq!(m.outcome(), Outcome::Won);
    }
}
#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d puzzles.but mines-controls: flag protection, 8 neighbours, matched-flag chord.
    #[test]
    fn recovered_mark_reveal_and_chord_transitions() {
        let mut m = Mines::new(0);
        m.placed = true;
        m.mines[0] = true;
        m.mines[1] = true;
        // Fixed source-derived two-mine fixture; other eight mine positions are irrelevant to this local transition.
        m.apply(PuzzleInput::Flag { cell: 0 }).unwrap();
        m.apply(PuzzleInput::Reveal { cell: 0 }).unwrap();
        assert_eq!(m.exploded, None);
        m.apply(PuzzleInput::Reveal { cell: 10 }).unwrap();
        assert_eq!(m.scene(0).cells[10], MineCell::Open(2));
        m.apply(PuzzleInput::Chord { cell: 10 }).unwrap();
        assert!(!m.revealed[11]);
        m.apply(PuzzleInput::Flag { cell: 1 }).unwrap();
        m.apply(PuzzleInput::Chord { cell: 10 }).unwrap();
        assert!(m.revealed[11]);
        assert_eq!(m.exploded, None);
        let initial = Mines::new(0).scene(0);
        assert_eq!(
            (initial.width, initial.height, initial.mine_count),
            (9, 9, 10)
        );
    }
}
#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn source_stock_dimensions_counts_and_first_click_safety() {
        for (width, height, mines) in [
            (9, 9, 10),
            (9, 9, 35),
            (16, 16, 40),
            (16, 16, 99),
            (30, 16, 99),
            (30, 16, 170),
        ] {
            let cfg = MinesConfig {
                width,
                height,
                mines,
                solvable: false,
            };
            let mut m = Mines::new_config(7, cfg).unwrap();
            m.apply(PuzzleInput::Reveal {
                cell: width * height / 2,
            })
            .unwrap();
            assert_eq!(m.mines.iter().filter(|&&v| v).count(), mines);
            let scene = m.scene(0);
            assert_eq!(
                (scene.width, scene.height, scene.mine_count),
                (width, height, mines)
            );
            assert!(!m.mines[width * height / 2]);
            m.solve().unwrap();
            assert_eq!(m.outcome(), Outcome::Won);
        }
    }
}
