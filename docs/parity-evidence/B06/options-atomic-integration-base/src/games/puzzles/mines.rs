use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineCell {
    Covered,
    Flagged,
    Open(u8),
    Exploded,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinesScene {
    pub width: usize,
    pub height: usize,
    pub mine_count: usize,
    pub cells: Vec<MineCell>,
    pub flags: usize,
    pub deaths: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mines {
    seed: u64,
    mines: Vec<bool>,
    revealed: Vec<bool>,
    flagged: Vec<bool>,
    placed: bool,
    exploded: Option<usize>,
}
fn number(mines: &[bool], i: usize) -> u8 {
    neighbours(i, 9, 9, true)
        .iter()
        .filter(|&&j| mines[j])
        .count() as u8
}
fn flood(mines: &[bool], revealed: &mut [bool], flagged: &[bool], start: usize) {
    let mut q = vec![start];
    while let Some(i) = q.pop() {
        if revealed[i] || flagged[i] || mines[i] {
            continue;
        }
        revealed[i] = true;
        if number(mines, i) == 0 {
            q.extend(neighbours(i, 9, 9, true));
        }
    }
}
/// Candidate acceptance uses deductions only: zero/all-mine sets and subset differences.
fn logical(mines: &[bool], start: usize) -> bool {
    let mut revealed = vec![false; 81];
    let mut flagged = vec![false; 81];
    flood(mines, &mut revealed, &flagged, start);
    for _ in 0..81 {
        if revealed.iter().filter(|&&v| v).count() == 71 {
            return true;
        }
        let mut constraints: Vec<(Vec<usize>, usize)> = Vec::new();
        for i in 0..81 {
            if revealed[i] {
                let around = neighbours(i, 9, 9, true);
                let known = around.iter().filter(|&&j| flagged[j]).count();
                let unknown: Vec<_> = around
                    .into_iter()
                    .filter(|&j| !revealed[j] && !flagged[j])
                    .collect();
                let n = number(mines, i) as usize;
                if n < known {
                    return false;
                }
                if !unknown.is_empty() {
                    constraints.push((unknown, n - known));
                }
            }
        }
        let unknown: Vec<_> = (0..81).filter(|&i| !revealed[i] && !flagged[i]).collect();
        constraints.push((unknown, 10 - flagged.iter().filter(|&&v| v).count()));
        let base = constraints.clone();
        for (a, na) in &base {
            for (b, nb) in &base {
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
                        flood(mines, &mut revealed, &flagged, i);
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
    #[cfg(test)]
    pub(super) fn mine_for_test(&self) -> usize {
        self.mines.iter().position(|&x| x).unwrap()
    }

    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            mines: vec![false; 81],
            revealed: vec![false; 81],
            flagged: vec![false; 81],
            placed: false,
            exploded: None,
        }
    }
    fn place(&mut self, start: usize) {
        let mut rng = Rng(self.seed ^ (start as u64).wrapping_mul(0x517cc1b727220a95));
        let mut excluded = neighbours(start, 9, 9, true);
        excluded.push(start);
        let mut candidates: Vec<_> = (0..81).filter(|i| !excluded.contains(i)).collect();
        for _ in 0..512 {
            rng.shuffle(&mut candidates);
            self.mines.fill(false);
            for &i in &candidates[..10] {
                self.mines[i] = true;
            }
            if logical(&self.mines, start) {
                self.placed = true;
                return;
            }
        }
        // A corner 2×5 block leaves one connected zero area revealing every safe cell.
        for top in [true, false] {
            for left in [true, false] {
                let cells: Vec<_> = (0..2)
                    .flat_map(|y| {
                        (0..5).map(move |x| {
                            (if top { y } else { 8 - y }) * 9 + if left { x } else { 8 - x }
                        })
                    })
                    .collect();
                if cells.iter().all(|i| !excluded.contains(i)) {
                    self.mines.fill(false);
                    for i in cells {
                        self.mines[i] = true;
                    }
                    self.placed = true;
                    return;
                }
            }
        }
        unreachable!("a 3×3 exclusion cannot touch all four corner blocks")
    }
    pub fn outcome(&self) -> Outcome {
        if self.exploded.is_some() {
            Outcome::Lost
        } else if self.placed && self.revealed.iter().filter(|&&v| v).count() == 71 {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self, deaths: u32) -> MinesScene {
        MinesScene {
            width: 9,
            height: 9,
            mine_count: 10,
            cells: (0..81)
                .map(|i| {
                    if self.exploded == Some(i) {
                        MineCell::Exploded
                    } else if self.revealed[i] {
                        MineCell::Open(number(&self.mines, i))
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
            flood(&self.mines, &mut self.revealed, &self.flagged, i);
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        let (cell, action) = match input {
            PuzzleInput::Reveal { cell } => (cell, 0),
            PuzzleInput::Flag { cell } => (cell, 1),
            PuzzleInput::Chord { cell } => (cell, 2),
            _ => return Err("Unsupported Mines input"),
        };
        if cell >= 81 {
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
            self.place(cell);
        }
        if self.revealed[cell] {
            let around = neighbours(cell, 9, 9, true);
            if around.iter().filter(|&&j| self.flagged[j]).count()
                == number(&self.mines, cell) as usize
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
