use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoloDifficulty {
    Trivial,
    Basic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoloSymmetry {
    None,
    Rotate2,
    Rotate4,
    Reflect2,
    ReflectDiagonal,
    Reflect4,
    Reflect4Diagonal,
    Reflect8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoloConfig {
    pub block_width: usize,
    pub block_height: usize,
    pub diagonal: bool,
    pub difficulty: SoloDifficulty,
    pub symmetry: SoloSymmetry,
}
impl Default for SoloConfig {
    fn default() -> Self {
        Self {
            block_width: 3,
            block_height: 3,
            diagonal: false,
            difficulty: SoloDifficulty::Trivial,
            symmetry: SoloSymmetry::Rotate2,
        }
    }
}
impl SoloConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if ![(2, 2), (2, 3), (3, 2), (3, 3), (3, 4), (4, 3), (4, 4)]
            .contains(&(self.block_width, self.block_height))
        {
            return Err("Unsupported Solo block dimensions");
        }
        if self.diagonal && self.block_width * self.block_height != 9 {
            return Err("Diagonal Solo currently supports 3x3 blocks");
        }
        Ok(())
    }
}
/// Complete source-rule replay; achieved reports the easiest supported solver
/// that completes this board. A Basic request can openly fall back to Trivial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoloCertificate {
    pub requested: SoloDifficulty,
    pub achieved: SoloDifficulty,
    pub easier_fallback: bool,
    pub block_steps: usize,
    pub basic_steps: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoloScene {
    pub width: usize,
    pub height: usize,
    pub block_width: usize,
    pub block_height: usize,
    pub diagonal: bool,
    pub symmetry: SoloSymmetry,
    pub certificate: SoloCertificate,
    pub digits: Vec<u8>,
    pub givens: Vec<bool>,
    pub pencil: Vec<u32>,
    pub conflicts: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Solo {
    config: SoloConfig,
    certificate: SoloCertificate,
    digits: Vec<u8>,
    givens: Vec<bool>,
    pencil: Vec<u32>,
    solution: Vec<u8>,
}
fn peers(i: usize, j: usize, c: SoloConfig) -> bool {
    let n = c.block_width * c.block_height;
    let (x, y) = (i % n, i / n);
    let (a, b) = (j % n, j / n);
    y == b
        || x == a
        || (x / c.block_width == a / c.block_width && y / c.block_height == b / c.block_height)
        || (c.diagonal && ((x == y && a == b) || (x + y == n - 1 && a + b == n - 1)))
}
fn allowed_config(d: &[u8], i: usize, c: SoloConfig) -> u32 {
    let n = c.block_width * c.block_height;
    let (x, y) = (i % n, i / n);
    let mut bits = ((1u32 << (n + 1)) - 1) & !1;
    for k in 0..n {
        bits &= !(1 << d[y * n + k]);
        bits &= !(1 << d[k * n + x]);
        let by = y / c.block_height * c.block_height + k / c.block_width;
        let bx = x / c.block_width * c.block_width + k % c.block_width;
        bits &= !(1 << d[by * n + bx]);
        if c.diagonal && x == y {
            bits &= !(1 << d[k * n + k]);
        }
        if c.diagonal && x + y == n - 1 {
            bits &= !(1 << d[k * n + n - 1 - k]);
        }
    }
    bits
}
#[cfg(test)]
fn allowed(d: &[u8], i: usize) -> u32 {
    allowed_config(d, i, SoloConfig::default())
}
#[cfg(test)]
fn count(d: &mut [u8], limit: usize) -> usize {
    search(d, limit, &mut 10_000)
}
#[cfg(test)]
fn search(d: &mut [u8], limit: usize, budget: &mut usize) -> usize {
    if *budget == 0 {
        return limit;
    }
    *budget -= 1;
    let best = (0..81)
        .filter(|&i| d[i] == 0)
        .map(|i| (i, allowed(d, i)))
        .min_by_key(|(_, b)| b.count_ones());
    let Some((i, bits)) = best else {
        return 1;
    };
    let mut found = 0;
    for n in 1..=9 {
        if bits & (1 << n) != 0 {
            d[i] = n;
            found += search(d, limit - found, budget);
            d[i] = 0;
            if found >= limit {
                return found;
            }
        }
    }
    found
}
fn units(c: SoloConfig, basic: bool) -> Vec<Vec<usize>> {
    let n = c.block_width * c.block_height;
    let mut u = Vec::new();
    for b in 0..n {
        u.push(
            (0..n)
                .map(|k| {
                    ((b / (n / c.block_width)) * c.block_height + k / c.block_width) * n
                        + (b % (n / c.block_width)) * c.block_width
                        + k % c.block_width
                })
                .collect(),
        );
    }
    if basic {
        for y in 0..n {
            u.push((0..n).map(|x| y * n + x).collect());
        }
        for x in 0..n {
            u.push((0..n).map(|y| y * n + x).collect());
        }
        if c.diagonal {
            u.push((0..n).map(|i| i * n + i).collect());
            u.push((0..n).map(|i| i * n + n - 1 - i).collect());
        }
    }
    u
}
// Historical DIFF_BLOCK: only box hidden singles. DIFF_SIMPLE additionally
// permits row/column/diagonal hidden singles and numeric (naked) singles.
fn deduction(input: &[u8], c: SoloConfig, max: SoloDifficulty) -> Option<(Vec<u8>, usize, usize)> {
    let n = c.block_width * c.block_height;
    let mut d = input.to_vec();
    let (mut block, mut basic) = (0, 0);
    let u = units(c, max == SoloDifficulty::Basic);
    loop {
        if !d.contains(&0) {
            return Some((d, block, basic));
        }
        let possible: Vec<_> = (0..d.len())
            .map(|i| {
                if d[i] == 0 {
                    allowed_config(&d, i, c)
                } else {
                    0
                }
            })
            .collect();
        if (0..d.len()).any(|i| d[i] == 0 && possible[i] == 0) {
            return None;
        }
        let mut step = None;
        'outer: for (which, unit) in u.iter().enumerate() {
            for digit in 1..=n as u8 {
                if unit.iter().any(|&i| d[i] == digit) {
                    continue;
                }
                let mut choices = unit
                    .iter()
                    .copied()
                    .filter(|&i| possible[i] & (1 << digit) != 0);
                let first = choices.next()?;
                if choices.next().is_none() {
                    step = Some((first, digit, which < n));
                    break 'outer;
                }
            }
        }
        if step.is_none() && max == SoloDifficulty::Basic {
            step = (0..d.len())
                .find(|&i| d[i] == 0 && possible[i].count_ones() == 1)
                .map(|i| (i, possible[i].trailing_zeros() as u8, false));
        }
        if let Some((i, digit, is_block)) = step {
            d[i] = digit;
            if is_block {
                block += 1;
            } else {
                basic += 1;
            }
        } else {
            return None;
        }
    }
}
fn orbit(i: usize, n: usize, s: SoloSymmetry) -> Vec<usize> {
    let (x, y) = (i % n, i / n);
    let m = n - 1;
    let coords = match s {
        SoloSymmetry::None => vec![(x, y)],
        SoloSymmetry::Rotate2 => vec![(x, y), (m - x, m - y)],
        SoloSymmetry::Rotate4 => vec![(x, y), (m - y, x), (y, m - x), (m - x, m - y)],
        SoloSymmetry::Reflect2 => vec![(x, y), (m - x, y)],
        SoloSymmetry::ReflectDiagonal => vec![(x, y), (y, x)],
        SoloSymmetry::Reflect4 => vec![(x, y), (m - x, y), (x, m - y), (m - x, m - y)],
        SoloSymmetry::Reflect4Diagonal => vec![(x, y), (y, x), (m - x, m - y), (m - y, m - x)],
        SoloSymmetry::Reflect8 => vec![
            (x, y),
            (m - x, y),
            (x, m - y),
            (m - x, m - y),
            (y, x),
            (y, m - x),
            (m - y, x),
            (m - y, m - x),
        ],
    };
    let mut out: Vec<_> = coords.into_iter().map(|(x, y)| y * n + x).collect();
    out.sort_unstable();
    out.dedup();
    out
}
fn diagonal_solution(d: &mut [u8], c: SoloConfig, rng: &mut Rng, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let best = (0..d.len())
        .filter(|&i| d[i] == 0)
        .map(|i| (i, allowed_config(d, i, c)))
        .min_by_key(|(_, b)| b.count_ones());
    let Some((i, bits)) = best else {
        return true;
    };
    let mut values: Vec<_> = (1..=(c.block_width * c.block_height) as u8)
        .filter(|v| bits & (1 << v) != 0)
        .collect();
    rng.shuffle(&mut values);
    for v in values {
        d[i] = v;
        if diagonal_solution(d, c, rng, budget) {
            return true;
        }
    }
    d[i] = 0;
    false
}
impl Solo {
    pub(super) fn solve(&mut self) {
        self.digits = self.solution.clone();
        self.pencil.fill(0);
    }
    pub fn new(rng: &mut Rng) -> Self {
        Self::new_config(rng, SoloConfig::default()).expect("valid default")
    }
    pub fn new_config(rng: &mut Rng, config: SoloConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let attempts = if config.difficulty == SoloDifficulty::Basic {
            8
        } else {
            1
        };
        let mut fallback = None;
        for _ in 0..attempts {
            let candidate = Self::candidate(rng, config)?;
            if !candidate.certificate.easier_fallback {
                return Ok(candidate);
            }
            fallback = Some(candidate);
        }
        Ok(fallback.expect("at least one certified candidate"))
    }
    fn candidate(rng: &mut Rng, config: SoloConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let n = config.block_width * config.block_height;
        let solution = if config.diagonal || (n == 6 && config.difficulty == SoloDifficulty::Basic)
        {
            let mut d = vec![0; n * n];
            if !diagonal_solution(&mut d, config, rng, &mut 100_000) {
                return Err("Diagonal Solo solution budget exhausted; try another seed");
            }
            d
        } else {
            let mut digit: Vec<_> = (1..=n as u8).collect();
            rng.shuffle(&mut digit);
            let mut bands: Vec<_> = (0..config.block_width).collect();
            let mut stacks: Vec<_> = (0..config.block_height).collect();
            rng.shuffle(&mut bands);
            rng.shuffle(&mut stacks);
            let mut rows = Vec::new();
            let mut cols = Vec::new();
            for b in bands {
                let mut a: Vec<_> = (0..config.block_height).collect();
                rng.shuffle(&mut a);
                rows.extend(a.into_iter().map(|v| b * config.block_height + v));
            }
            for b in stacks {
                let mut a: Vec<_> = (0..config.block_width).collect();
                rng.shuffle(&mut a);
                cols.extend(a.into_iter().map(|v| b * config.block_width + v));
            }
            let mut d = Vec::new();
            for &r in &rows {
                for &col in &cols {
                    d.push(digit[(r * config.block_width + r / config.block_height + col) % n]);
                }
            }
            d
        };
        let mut digits = solution.clone();
        let mut order: Vec<_> = (0..n * n).collect();
        rng.shuffle(&mut order);
        let mut seen = vec![false; n * n];
        for i in order {
            if seen[i] {
                continue;
            }
            let cells = orbit(i, n, config.symmetry);
            let old: Vec<_> = cells.iter().map(|&i| digits[i]).collect();
            for &i in &cells {
                seen[i] = true;
                digits[i] = 0;
            }
            if deduction(&digits, config, config.difficulty).is_none() {
                for (&i, &v) in cells.iter().zip(&old) {
                    digits[i] = v;
                }
            }
        }
        let (achieved, (_, block_steps, basic_steps)) =
            if let Some(r) = deduction(&digits, config, SoloDifficulty::Trivial) {
                (SoloDifficulty::Trivial, r)
            } else {
                (
                    SoloDifficulty::Basic,
                    deduction(&digits, config, SoloDifficulty::Basic)
                        .expect("accepted removals retain complete replay"),
                )
            };
        let certificate = SoloCertificate {
            requested: config.difficulty,
            achieved,
            easier_fallback: achieved != config.difficulty,
            block_steps,
            basic_steps,
        };
        let givens = digits.iter().map(|&d| d != 0).collect();
        Ok(Self {
            config,
            certificate,
            digits,
            givens,
            pencil: vec![0; n * n],
            solution,
        })
    }
    fn conflicts(&self) -> Vec<bool> {
        (0..self.digits.len())
            .map(|i| {
                self.digits[i] != 0
                    && (0..self.digits.len()).any(|j| {
                        i != j && self.digits[j] == self.digits[i] && peers(i, j, self.config)
                    })
            })
            .collect()
    }
    pub fn outcome(&self) -> Outcome {
        if !self.digits.contains(&0) && !self.conflicts().contains(&true) {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> SoloScene {
        let n = self.config.block_width * self.config.block_height;
        SoloScene {
            width: n,
            height: n,
            block_width: self.config.block_width,
            block_height: self.config.block_height,
            diagonal: self.config.diagonal,
            symmetry: self.config.symmetry,
            certificate: self.certificate,
            digits: self.digits.clone(),
            givens: self.givens.clone(),
            pencil: self.pencil.clone(),
            conflicts: self.conflicts(),
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        let n = self.config.block_width * self.config.block_height;
        match input {
            PuzzleInput::Digit { cell, value } => {
                if cell >= self.digits.len() || value as usize > n {
                    return Err("Digit outside board/range");
                }
                if self.givens[cell] {
                    return Err("Starting digit is fixed");
                }
                self.digits[cell] = value;
                self.pencil[cell] = 0;
            }
            PuzzleInput::Pencil { cell, value } => {
                if cell >= self.digits.len() || value == 0 || value as usize > n {
                    return Err("Pencil digit outside board/range");
                }
                if self.givens[cell] || self.digits[cell] != 0 {
                    return Err("Pencil marks require an empty editable square");
                }
                self.pencil[cell] ^= 1 << value;
            }
            _ => return Err("Unsupported Solo input"),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_unique_and_solvable() {
        for seed in 0..20 {
            let mut s = Solo::new(&mut Rng(seed));
            assert_eq!(count(&mut s.digits.clone(), 2), 1);
            assert!(s.digits.iter().filter(|&&v| v == 0).count() >= 40);
            let i = s.givens.iter().position(|&v| !v).unwrap();
            s.apply(PuzzleInput::Pencil { cell: i, value: 4 }).unwrap();
            assert_ne!(s.pencil[i], 0);
            for i in 0..81 {
                if !s.givens[i] {
                    s.apply(PuzzleInput::Digit {
                        cell: i,
                        value: s.solution[i],
                    })
                    .unwrap();
                }
            }
            assert_eq!(s.outcome(), Outcome::Won);
        }
    }
    #[test]
    fn duplicate_is_visible_not_win() {
        let mut s = Solo::new(&mut Rng(77));
        let i = s.givens.iter().position(|&v| !v).unwrap();
        let j = (i / 9 * 9..i / 9 * 9 + 9).find(|&j| s.givens[j]).unwrap();
        s.apply(PuzzleInput::Digit {
            cell: i,
            value: s.digits[j],
        })
        .unwrap();
        assert!(s.scene().conflicts[i]);
        assert_eq!(s.outcome(), Outcome::Playing);
    }
}
#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d puzzles.but solo + solo-controls: row, column, 3×3 constraints and zero erase.
    #[test]
    fn recovered_three_units_and_erase() {
        for other in [8, 72, 10] {
            let mut s = Solo {
                config: SoloConfig::default(),
                certificate: SoloCertificate {
                    requested: SoloDifficulty::Trivial,
                    achieved: SoloDifficulty::Trivial,
                    easier_fallback: false,
                    block_steps: 0,
                    basic_steps: 0,
                },
                digits: vec![0; 81],
                givens: vec![false; 81],
                pencil: vec![0; 81],
                solution: Vec::new(),
            };
            s.apply(PuzzleInput::Digit { cell: 0, value: 7 }).unwrap();
            s.apply(PuzzleInput::Digit {
                cell: other,
                value: 7,
            })
            .unwrap();
            assert!(s.conflicts()[0]);
            s.apply(PuzzleInput::Digit {
                cell: other,
                value: 0,
            })
            .unwrap();
            assert!(!s.conflicts()[0]);
            assert_eq!(s.digits[other], 0);
            s.givens[0] = true;
            assert!(s.apply(PuzzleInput::Digit { cell: 0, value: 0 }).is_err());
        }
        let s = Solo::new(&mut Rng(0)).scene();
        assert_eq!((s.width, s.height), (9, 9));
    }
}

#[cfg(test)]
mod options_tests {
    use super::*;
    #[test]
    fn default_clues_have_half_turn_symmetry() {
        for seed in 0..32 {
            let s = Solo::new(&mut Rng(seed));
            for i in 0..81 {
                assert_eq!(s.givens[i], s.givens[80 - i], "seed {seed} cell {i}");
            }
        }
    }
    #[test]
    fn default_is_solved_using_only_block_position_elimination() {
        for seed in 0..32 {
            let s = Solo::new(&mut Rng(seed));
            let mut digits = s.digits.clone();
            // Independent fixture replay; no row/column hidden singles or naked singles.
            loop {
                let before = digits.clone();
                for b in 0..9 {
                    for n in 1..=9 {
                        let cells: Vec<usize> = (0..9)
                            .map(|k| (b / 3 * 3 + k / 3) * 9 + b % 3 * 3 + k % 3)
                            .collect();
                        if cells.iter().any(|&i| digits[i] == n) {
                            continue;
                        }
                        let choices: Vec<_> = cells
                            .into_iter()
                            .filter(|&i| digits[i] == 0 && allowed(&digits, i) & (1 << n) != 0)
                            .collect();
                        if choices.len() == 1 {
                            digits[choices[0]] = n;
                        }
                    }
                }
                if digits == before {
                    break;
                }
            }
            assert_eq!(digits, s.solution, "seed {seed}");
        }
    }
}
#[cfg(test)]
mod extended_configuration_tests {
    use super::*;
    #[test]
    fn stock_basic_and_small_sizes_are_logically_graded() {
        for (block_width, block_height, diagonal) in [
            (2, 2, false),
            (3, 2, false),
            (3, 3, false),
            (3, 3, true),
            (4, 3, false),
            (4, 4, false),
        ] {
            let cfg = SoloConfig {
                block_width,
                block_height,
                diagonal,
                difficulty: SoloDifficulty::Basic,
                symmetry: SoloSymmetry::Rotate2,
            };
            let mut s = Solo::new_config(&mut Rng(19), cfg).unwrap();
            let n = block_width * block_height;
            assert_eq!((s.scene().width, s.scene().height), (n, n));
            assert!(deduction(&s.digits, cfg, SoloDifficulty::Basic).is_some());
            assert_eq!(
                s.scene().certificate.achieved,
                if deduction(&s.digits, cfg, SoloDifficulty::Trivial).is_some() {
                    SoloDifficulty::Trivial
                } else {
                    SoloDifficulty::Basic
                }
            );
            if diagonal {
                for i in 0..n {
                    for j in i + 1..n {
                        assert_ne!(s.solution[i * n + i], s.solution[j * n + j]);
                        assert_ne!(s.solution[i * n + n - 1 - i], s.solution[j * n + n - 1 - j]);
                    }
                }
            }
            let editable = s.givens.iter().position(|&v| !v).unwrap();
            s.apply(PuzzleInput::Pencil {
                cell: editable,
                value: n as u8,
            })
            .unwrap();
            assert_eq!(s.pencil[editable], 1u32 << n);
            s.solve();
            assert_eq!(s.outcome(), Outcome::Won);
        }
    }
    #[test]
    fn every_source_symmetry_is_atomic_and_diagonal_conflicts_enforced() {
        for symmetry in [
            SoloSymmetry::None,
            SoloSymmetry::Rotate2,
            SoloSymmetry::Rotate4,
            SoloSymmetry::Reflect2,
            SoloSymmetry::ReflectDiagonal,
            SoloSymmetry::Reflect4,
            SoloSymmetry::Reflect4Diagonal,
            SoloSymmetry::Reflect8,
        ] {
            let cfg = SoloConfig {
                symmetry,
                ..SoloConfig::default()
            };
            let s = Solo::new_config(&mut Rng(4), cfg).unwrap();
            for i in 0..81 {
                for j in orbit(i, 9, symmetry) {
                    assert_eq!(s.givens[i], s.givens[j]);
                }
            }
        }
        let cfg = SoloConfig {
            diagonal: true,
            ..SoloConfig::default()
        };
        let mut s = Solo::new_config(&mut Rng(4), cfg).unwrap();
        s.digits.fill(0);
        s.givens.fill(false);
        s.digits[0] = 1;
        s.digits[40] = 1;
        assert!(s.conflicts()[0]);
        assert!(s.conflicts()[40]);
    }
}
