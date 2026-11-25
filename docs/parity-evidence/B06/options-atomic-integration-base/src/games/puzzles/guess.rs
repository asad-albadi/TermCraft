use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuessRow {
    pub pegs: [u8; 4],
    pub exact: u8,
    pub misplaced: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuessScene {
    pub colours: u8,
    pub max_guesses: usize,
    pub current: [u8; 4],
    pub held: [bool; 4],
    pub rows: Vec<GuessRow>,
    pub solution: Option<[u8; 4]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Guess {
    solution: [u8; 4],
    current: [u8; 4],
    held: [bool; 4],
    rows: Vec<GuessRow>,
}
fn feedback(a: [u8; 4], b: [u8; 4]) -> (u8, u8) {
    let mut exact = 0;
    let mut ca = [0u8; 7];
    let mut cb = [0u8; 7];
    for i in 0..4 {
        if a[i] == b[i] {
            exact += 1;
        } else {
            ca[a[i] as usize] += 1;
            cb[b[i] as usize] += 1;
        }
    }
    (exact, (1..=6).map(|i| ca[i].min(cb[i])).sum())
}
impl Guess {
    #[cfg(test)]
    pub(super) fn solution_for_test(&self) -> [u8; 4] {
        self.solution
    }

    pub fn new(rng: &mut Rng) -> Self {
        Self {
            solution: std::array::from_fn(|_| rng.below(6) as u8 + 1),
            current: [0; 4],
            held: [false; 4],
            rows: Vec::new(),
        }
    }
    pub fn outcome(&self) -> Outcome {
        if self.rows.last().is_some_and(|r| r.exact == 4) {
            Outcome::Won
        } else if self.rows.len() == 10 {
            Outcome::Lost
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> GuessScene {
        GuessScene {
            colours: 6,
            max_guesses: 10,
            current: self.current,
            held: self.held,
            rows: self.rows.clone(),
            solution: if self.outcome() != Outcome::Playing {
                Some(self.solution)
            } else {
                None
            },
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        match input {
            PuzzleInput::Peg { slot, colour } => {
                if slot >= 4 || colour > 6 {
                    return Err("Peg outside tray");
                }
                self.current[slot] = colour;
            }
            PuzzleInput::Hold { slot } => {
                if slot >= 4 {
                    return Err("Peg outside row");
                }
                self.held[slot] = !self.held[slot];
            }
            PuzzleInput::Submit => {
                if self.current.contains(&0) {
                    return Err("Fill all four pegs before marking");
                }
                let (exact, misplaced) = feedback(self.solution, self.current);
                self.rows.push(GuessRow {
                    pegs: self.current,
                    exact,
                    misplaced,
                });
                for i in 0..4 {
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
        s.current = s.solution;
        s.apply(PuzzleInput::Submit).unwrap();
        assert_eq!(s.outcome(), Outcome::Won);
        let mut s = Guess::new(&mut Rng(9));
        let wrong = s.solution[0] % 6 + 1;
        for _ in 0..10 {
            s.current = [wrong; 4];
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
        g.solution = [1, 1, 2, 3];
        g.current = [1, 2, 1, 1];
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
