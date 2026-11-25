use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoloScene {
    pub width: usize,
    pub height: usize,
    pub digits: Vec<u8>,
    pub givens: Vec<bool>,
    pub pencil: Vec<u16>,
    pub conflicts: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Solo {
    digits: Vec<u8>,
    givens: Vec<bool>,
    pencil: Vec<u16>,
    solution: Vec<u8>,
}
fn allowed(d: &[u8], i: usize) -> u16 {
    let (x, y) = (i % 9, i / 9);
    let mut bits = 0x3fe;
    for n in 0..9 {
        bits &= !(1 << d[y * 9 + n]);
        bits &= !(1 << d[n * 9 + x]);
        bits &= !(1 << d[(y / 3 * 3 + n / 3) * 9 + x / 3 * 3 + n % 3]);
    }
    bits
}
fn count(d: &mut [u8], limit: usize) -> usize {
    search(d, limit, &mut 10_000)
}
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
impl Solo {
    pub fn new(rng: &mut Rng) -> Self {
        let mut digit = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        rng.shuffle(&mut digit);
        let mut bands = [0, 1, 2];
        let mut stacks = [0, 1, 2];
        rng.shuffle(&mut bands);
        rng.shuffle(&mut stacks);
        let mut rows = Vec::new();
        let mut cols = Vec::new();
        for b in bands {
            let mut a = [0, 1, 2];
            rng.shuffle(&mut a);
            rows.extend(a.map(|v| b * 3 + v));
        }
        for b in stacks {
            let mut a = [0, 1, 2];
            rng.shuffle(&mut a);
            cols.extend(a.map(|v| b * 3 + v));
        }
        let mut solution = Vec::new();
        for &r in &rows {
            for &c in &cols {
                solution.push(digit[(r * 3 + r / 3 + c) % 9]);
            }
        }
        let mut digits = solution.clone();
        let mut order: Vec<_> = (0..81).collect();
        rng.shuffle(&mut order);
        let mut removed = 0;
        for i in order {
            let old = digits[i];
            digits[i] = 0;
            if count(&mut digits, 2) != 1 {
                digits[i] = old;
            } else {
                removed += 1;
            }
            if removed == 45 {
                break;
            }
        }
        let givens = digits.iter().map(|&v| v != 0).collect();
        Self {
            digits,
            givens,
            pencil: vec![0; 81],
            solution,
        }
    }
    fn conflicts(&self) -> Vec<bool> {
        (0..81)
            .map(|i| {
                let n = self.digits[i];
                n != 0
                    && (0..81).any(|j| {
                        i != j
                            && self.digits[j] == n
                            && (i / 9 == j / 9
                                || i % 9 == j % 9
                                || (i / 27 == j / 27 && i % 9 / 3 == j % 9 / 3))
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
        SoloScene {
            width: 9,
            height: 9,
            digits: self.digits.clone(),
            givens: self.givens.clone(),
            pencil: self.pencil.clone(),
            conflicts: self.conflicts(),
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        match input {
            PuzzleInput::Digit { cell, value } => {
                if cell >= 81 || value > 9 {
                    return Err("Digit outside board/range");
                }
                if self.givens[cell] {
                    return Err("Starting digit is fixed");
                }
                self.digits[cell] = value;
                self.pencil[cell] = 0;
            }
            PuzzleInput::Pencil { cell, value } => {
                if cell >= 81 || !(1..=9).contains(&value) {
                    return Err("Pencil digit outside board/range");
                }
                if self.givens[cell] || self.digits[cell] != 0 {
                    return Err("Pencil marks require an empty editable square");
                }
                self.pencil[cell] ^= 1 << value;
            }
            _ => return Err("Unsupported Solo input"),
        };
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
