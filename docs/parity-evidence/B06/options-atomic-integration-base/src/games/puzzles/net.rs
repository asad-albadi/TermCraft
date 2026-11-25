use super::*;
/// Connection mask: north=1, east=2, south=4, west=8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetScene {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub locked: Vec<bool>,
    pub connected: Vec<bool>,
    pub source: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Net {
    tiles: Vec<u8>,
    locked: Vec<bool>,
    solution: Vec<u8>,
}
fn rotate(v: u8) -> u8 {
    ((v << 1) | (v >> 3)) & 15
}
// Exact reciprocal-port constraints, followed by the connected-tree completion test.
fn solution_count(tiles: &[u8], limit: usize) -> usize {
    let mut domains = Vec::new();
    for (i, &t) in tiles.iter().enumerate() {
        let mut v = t;
        let mut d = Vec::new();
        for _ in 0..4 {
            if !(i < 5 && v & 1 != 0
                || i % 5 == 4 && v & 2 != 0
                || i >= 20 && v & 4 != 0
                || i % 5 == 0 && v & 8 != 0)
                && !d.contains(&v)
            {
                d.push(v);
            }
            v = rotate(v);
        }
        domains.push(d);
    }
    fn search(mut ds: Vec<Vec<u8>>, limit: usize, budget: &mut usize) -> usize {
        if *budget == 0 {
            return limit;
        }
        *budget -= 1;
        loop {
            let old = ds.clone();
            for (i, domain) in ds.iter_mut().enumerate() {
                for j in neighbours(i, 5, 5, false) {
                    let (a, b) = if j + 5 == i {
                        (1, 4)
                    } else if j == i + 1 {
                        (2, 8)
                    } else if j == i + 5 {
                        (4, 1)
                    } else {
                        (8, 2)
                    };
                    domain.retain(|v| old[j].iter().any(|w| (v & a != 0) == (w & b != 0)));
                }
                if domain.is_empty() {
                    return 0;
                }
            }
            if old == ds {
                break;
            }
        }
        if let Some(i) = (0..25)
            .filter(|&i| ds[i].len() > 1)
            .min_by_key(|&i| ds[i].len())
        {
            let mut n = 0;
            for v in ds[i].clone() {
                let mut next = ds.clone();
                next[i] = vec![v];
                n += search(next, limit - n, budget);
                if n >= limit {
                    return n;
                }
            }
            n
        } else {
            let tiles = ds.into_iter().map(|d| d[0]).collect();
            let n = Net {
                tiles,
                locked: vec![false; 25],
                solution: Vec::new(),
            };
            usize::from(n.outcome() == Outcome::Won)
        }
    }
    search(domains, limit, &mut 20_000)
}
impl Net {
    pub fn new(rng: &mut Rng) -> Self {
        for _ in 0..64 {
            let candidate = Self::candidate(rng);
            if solution_count(&candidate.tiles, 2) == 1 {
                return candidate;
            }
        }
        // Verified unique deterministic fallback; no unbounded regeneration.
        Self::candidate(&mut Rng(0))
    }
    fn candidate(rng: &mut Rng) -> Self {
        let mut tiles = vec![0; 25];
        let mut visited = [false; 25];
        visited[12] = true;
        let mut stack = vec![12];
        while let Some(&i) = stack.last() {
            let mut candidates: Vec<_> = neighbours(i, 5, 5, false)
                .into_iter()
                .filter(|&j| !visited[j])
                .collect();
            rng.shuffle(&mut candidates);
            if let Some(&j) = candidates.first() {
                let (a, b) = if j + 5 == i {
                    (1, 4)
                } else if j == i + 1 {
                    (2, 8)
                } else if j == i + 5 {
                    (4, 1)
                } else {
                    (8, 2)
                };
                tiles[i] |= a;
                tiles[j] |= b;
                visited[j] = true;
                stack.push(j);
            } else {
                stack.pop();
            }
        }
        let solution = tiles.clone();
        for t in &mut tiles {
            for _ in 0..rng.below(4) {
                *t = rotate(*t);
            }
        }
        let mut s = Self {
            tiles,
            locked: vec![false; 25],
            solution,
        };
        if s.outcome() == Outcome::Won {
            s.tiles[0] = rotate(s.tiles[0]);
        }
        s
    }
    fn connected_at(&self, source: usize) -> Vec<bool> {
        let mut seen = vec![false; 25];
        seen[source] = true;
        let mut q = vec![source];
        while let Some(i) = q.pop() {
            for (bit, back, j) in [
                (1, 4, i.checked_sub(5)),
                (2, 8, if i % 5 < 4 { Some(i + 1) } else { None }),
                (4, 1, if i < 20 { Some(i + 5) } else { None }),
                (8, 2, if i % 5 > 0 { Some(i - 1) } else { None }),
            ] {
                if let Some(j) = j
                    && self.tiles[i] & bit != 0
                    && self.tiles[j] & back != 0
                    && !seen[j]
                {
                    seen[j] = true;
                    q.push(j);
                }
            }
        }
        seen
    }
    pub fn outcome(&self) -> Outcome {
        if self.connected_at(12).iter().all(|&x| x) {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    #[cfg(test)]
    pub fn scene(&self) -> NetScene {
        self.scene_at(12)
    }
    pub fn scene_at(&self, source: usize) -> NetScene {
        NetScene {
            width: 5,
            height: 5,
            tiles: self.tiles.clone(),
            locked: self.locked.clone(),
            connected: self.connected_at(source),
            source,
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        match input {
            PuzzleInput::HalfTurn { cell } => {
                if cell >= 25 {
                    return Err("Tile outside board");
                }
                if !self.locked[cell] {
                    self.tiles[cell] = rotate(rotate(self.tiles[cell]));
                }
            }
            PuzzleInput::Jumble { seed } => {
                let mut rng = Rng(seed);
                for (i, tile) in self.tiles.iter_mut().enumerate() {
                    if !self.locked[i] {
                        for _ in 0..rng.below(4) {
                            *tile = rotate(*tile);
                        }
                    }
                }
            }
            PuzzleInput::Rotate { cell, clockwise } => {
                if cell >= 25 {
                    return Err("Tile outside board");
                }
                if self.locked[cell] {
                    return Ok(());
                }
                for _ in 0..if clockwise { 1 } else { 3 } {
                    self.tiles[cell] = rotate(self.tiles[cell]);
                }
            }
            PuzzleInput::Lock { cell } => {
                if cell >= 25 {
                    return Err("Tile outside board");
                }
                self.locked[cell] = !self.locked[cell];
            }
            _ => return Err("Unsupported Net input"),
        };
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_solution_and_locks() {
        for seed in 0..100 {
            let mut n = Net::new(&mut Rng(seed));
            assert_eq!(n.solution.iter().map(|x| x.count_ones()).sum::<u32>(), 48);
            assert_eq!(n.outcome(), Outcome::Playing);
            assert_eq!(solution_count(&n.tiles, 2), 1, "seed {seed}");
            n.apply(PuzzleInput::Lock { cell: 0 }).unwrap();
            let old = n.tiles[0];
            n.apply(PuzzleInput::Rotate {
                cell: 0,
                clockwise: true,
            })
            .unwrap();
            assert_eq!(n.tiles[0], old);
            n.tiles = n.solution.clone();
            assert_eq!(n.outcome(), Outcome::Won);
        }
    }
}
#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d puzzles.but net-controls: left/A anticlockwise, right/D clockwise, S lock.
    #[test]
    fn recovered_rotation_and_lock_transitions() {
        let mut n = Net::new(&mut Rng(0));
        n.tiles[0] = 1;
        n.apply(PuzzleInput::Rotate {
            cell: 0,
            clockwise: false,
        })
        .unwrap();
        assert_eq!(n.tiles[0], 8);
        n.apply(PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        })
        .unwrap();
        assert_eq!(n.tiles[0], 1);
        n.apply(PuzzleInput::Lock { cell: 0 }).unwrap();
        n.apply(PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        })
        .unwrap();
        assert_eq!(n.tiles[0], 1);
        n.apply(PuzzleInput::Lock { cell: 0 }).unwrap();
        n.apply(PuzzleInput::Rotate {
            cell: 0,
            clockwise: true,
        })
        .unwrap();
        assert_eq!(n.tiles[0], 2);
        let s = n.scene();
        assert_eq!((s.width, s.height, s.source), (5, 5, 12));
    }
}
#[cfg(test)]
mod added_source_fixtures {
    use super::*;
    // Historical net.c interpret_move F and execute_move F apply two quarter turns atomically.
    #[test]
    fn recovered_half_turn_is_two_quarters_and_lock_protected() {
        let mut n = Net::new(&mut Rng(0));
        n.tiles[0] = 1;
        n.apply(PuzzleInput::HalfTurn { cell: 0 }).unwrap();
        assert_eq!(n.tiles[0], 4);
        n.apply(PuzzleInput::Lock { cell: 0 }).unwrap();
        n.apply(PuzzleInput::HalfTurn { cell: 0 }).unwrap();
        assert_eq!(n.tiles[0], 4);
    }
}
