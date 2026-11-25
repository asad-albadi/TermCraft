use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuessConfig {
    pub colours: u8,
    pub pegs: usize,
    pub max_guesses: usize,
    pub allow_blank: bool,
    pub allow_multiple: bool,
}
impl Default for GuessConfig {
    fn default() -> Self {
        Self {
            colours: 6,
            pegs: 4,
            max_guesses: 10,
            allow_blank: false,
            allow_multiple: true,
        }
    }
}
impl GuessConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if !(2..=10).contains(&self.colours)
            || !(2..=8).contains(&self.pegs)
            || !(1..=32).contains(&self.max_guesses)
        {
            return Err("Guess supports 2–10 colours, 2–8 pegs and 1–32 guesses");
        }
        if !self.allow_multiple && (self.colours as usize) < self.pegs {
            return Err("Distinct pegs require at least as many colours as pegs");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuessRow {
    pub pegs: Vec<u8>,
    pub exact: u8,
    pub misplaced: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuessScene {
    pub colours: u8,
    pub max_guesses: usize,
    pub current: Vec<u8>,
    pub held: Vec<bool>,
    pub rows: Vec<GuessRow>,
    pub solution: Option<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Guess {
    config: GuessConfig,
    gave_up: bool,
    solution: Vec<u8>,
    current: Vec<u8>,
    held: Vec<bool>,
    rows: Vec<GuessRow>,
}
fn feedback(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> (u8, u8) {
    let (a, b) = (a.as_ref(), b.as_ref());
    let mut exact = 0;
    let mut ca = [0u8; 11];
    let mut cb = [0u8; 11];
    for i in 0..a.len() {
        if a[i] == b[i] {
            exact += 1;
        } else {
            ca[a[i] as usize] += 1;
            cb[b[i] as usize] += 1;
        }
    }
    (exact, (1..=10).map(|i| ca[i].min(cb[i])).sum())
}
impl Guess {
    pub(super) fn solve(&mut self) {
        self.gave_up = true;
    }
    #[cfg(test)]
    pub(super) fn solution_for_test(&self) -> Vec<u8> {
        self.solution.clone()
    }

    pub fn new(rng: &mut Rng) -> Self {
        Self::new_config(rng, GuessConfig::default()).expect("valid default")
    }
    pub fn new_config(rng: &mut Rng, config: GuessConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let solution = if config.allow_multiple {
            (0..config.pegs)
                .map(|_| rng.below(config.colours as usize) as u8 + 1)
                .collect()
        } else {
            let mut colours: Vec<_> = (1..=config.colours).collect();
            rng.shuffle(&mut colours);
            colours.truncate(config.pegs);
            colours
        };
        Ok(Self {
            config,
            gave_up: false,
            solution,
            current: vec![0; config.pegs],
            held: vec![false; config.pegs],
            rows: Vec::new(),
        })
    }
    pub fn outcome(&self) -> Outcome {
        if self.gave_up {
            Outcome::Lost
        } else if self
            .rows
            .last()
            .is_some_and(|r| r.exact as usize == self.config.pegs)
        {
            Outcome::Won
        } else if self.rows.len() == self.config.max_guesses {
            Outcome::Lost
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> GuessScene {
        GuessScene {
            colours: self.config.colours,
            max_guesses: self.config.max_guesses,
            current: self.current.clone(),
            held: self.held.clone(),
            rows: self.rows.clone(),
            solution: if self.outcome() != Outcome::Playing {
                Some(self.solution.clone())
            } else {
                None
            },
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        match input {
            PuzzleInput::Peg { slot, colour } => {
                if slot >= self.config.pegs || colour > self.config.colours {
                    return Err("Peg outside tray");
                }
                self.current[slot] = colour;
            }
            PuzzleInput::Hold { slot } => {
                if slot >= self.config.pegs {
                    return Err("Peg outside row");
                }
                self.held[slot] = !self.held[slot];
            }
            PuzzleInput::Submit => {
                if (!self.config.allow_blank && self.current.contains(&0))
                    || self.current.iter().all(|&c| c == 0)
                {
                    return Err("Complete the guess under the selected blank rules before marking");
                }
                if !self.config.allow_multiple
                    && (1..=self.config.colours)
                        .any(|c| self.current.iter().filter(|&&v| v == c).count() > 1)
                {
                    return Err("Repeated colours are disabled");
                }
                let (exact, misplaced) = feedback(&self.solution, &self.current);
                self.rows.push(GuessRow {
                    pegs: self.current.clone(),
                    exact,
                    misplaced,
                });
                for i in 0..self.config.pegs {
                    if !self.held[i] {
                        self.current[i] = 0;
                    }
                }
            }
            _ => return Err("Unsupported Guess input"),
        };
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicates_not_double_counted() {
        assert_eq!(feedback([1, 1, 2, 3], [1, 2, 1, 1]), (1, 2));
        assert_eq!(feedback([1, 1, 1, 1], [2, 2, 2, 1]), (1, 0));
        assert_eq!(feedback([1, 2, 3, 4], [4, 3, 2, 1]), (0, 4));
    }
    #[test]
    fn wins_and_loses() {
        let mut s = Guess::new(&mut Rng(6));
        s.current = s.solution.clone();
        s.apply(PuzzleInput::Submit).unwrap();
        assert_eq!(s.outcome(), Outcome::Won);
        let mut s = Guess::new(&mut Rng(9));
        let wrong = s.solution[0] % 6 + 1;
        for _ in 0..10 {
            s.current = vec![wrong; 4];
            s.apply(PuzzleInput::Submit).unwrap();
        }
        assert_eq!(s.outcome(), Outcome::Lost);
        assert_eq!(s.scene().solution, Some(s.solution));
    }
}
#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d guess.c mark_pegs and default_params; puzzles.but guess-controls hold transfer.
    #[test]
    fn recovered_duplicate_feedback_and_hold_transfer() {
        let mut g = Guess::new(&mut Rng(0));
        g.solution = vec![1, 1, 2, 3];
        g.current = vec![1, 2, 1, 1];
        g.apply(PuzzleInput::Hold { slot: 1 }).unwrap();
        g.apply(PuzzleInput::Submit).unwrap();
        assert_eq!((g.rows[0].exact, g.rows[0].misplaced), (1, 2));
        assert_eq!(g.current, [0, 2, 0, 0]);
        let scene = g.scene();
        assert_eq!(
            (scene.colours, scene.max_guesses, scene.current.len()),
            (6, 10, 4)
        );
        assert_eq!(scene.solution, None);
    }
}
#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn super_and_blank_distinct_rules() {
        let mut g = Guess::new_config(
            &mut Rng(4),
            GuessConfig {
                colours: 8,
                pegs: 5,
                max_guesses: 12,
                allow_blank: false,
                allow_multiple: true,
            },
        )
        .unwrap();
        assert_eq!(g.scene().current.len(), 5);
        assert_eq!(g.scene().colours, 8);
        assert_eq!(g.scene().max_guesses, 12);
        g.current = g.solution.clone();
        g.apply(PuzzleInput::Submit).unwrap();
        assert_eq!(g.outcome(), Outcome::Won);
        let cfg = GuessConfig {
            colours: 6,
            pegs: 4,
            max_guesses: 10,
            allow_blank: true,
            allow_multiple: false,
        };
        let mut g = Guess::new_config(&mut Rng(1), cfg).unwrap();
        assert!(g.apply(PuzzleInput::Submit).is_err());
        g.current = vec![2, 2, 0, 0];
        assert!(g.apply(PuzzleInput::Submit).is_err());
        g.current = vec![2, 0, 0, 0];
        g.apply(PuzzleInput::Submit).unwrap();
        assert_eq!(g.rows.len(), 1);
        assert_eq!(feedback([4, 1, 2, 3], [0, 2, 1, 0]), (0, 2));
    }
}
