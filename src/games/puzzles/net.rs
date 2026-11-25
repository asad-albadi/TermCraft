use super::*;
/// Supported bounds are explicit resource limits, not upstream format limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetConfig {
    pub width: usize,
    pub height: usize,
    pub wrapping: bool,
    pub barrier_percent: u8,
    pub unique: bool,
}
impl Default for NetConfig {
    fn default() -> Self {
        Self {
            width: 5,
            height: 5,
            wrapping: false,
            barrier_percent: 0,
            unique: true,
        }
    }
}
impl NetConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if !(3..=16).contains(&self.width) || !(3..=16).contains(&self.height) {
            return Err("Net supports dimensions 3 through 16");
        }
        if self.barrier_percent > 100 {
            return Err("Barrier percentage exceeds 100");
        }
        Ok(())
    }
}
/// Connection/barrier masks: north=1, east=2, south=4, west=8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetScene {
    pub width: usize,
    pub height: usize,
    pub wrapping: bool,
    pub barriers: Vec<u8>,
    pub tiles: Vec<u8>,
    pub locked: Vec<bool>,
    pub connected: Vec<bool>,
    pub source: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Net {
    config: NetConfig,
    tiles: Vec<u8>,
    barriers: Vec<u8>,
    locked: Vec<bool>,
    solution: Vec<u8>,
}
fn rotate(v: u8) -> u8 {
    ((v << 1) | (v >> 3)) & 15
}
fn ports(i: usize, c: NetConfig) -> [(u8, u8, Option<usize>); 4] {
    let (x, y) = (i % c.width, i / c.width);
    [
        (
            1,
            4,
            if y > 0 {
                Some(i - c.width)
            } else if c.wrapping {
                Some((c.height - 1) * c.width + x)
            } else {
                None
            },
        ),
        (
            2,
            8,
            if x + 1 < c.width {
                Some(i + 1)
            } else if c.wrapping {
                Some(y * c.width)
            } else {
                None
            },
        ),
        (
            4,
            1,
            if y + 1 < c.height {
                Some(i + c.width)
            } else if c.wrapping {
                Some(x)
            } else {
                None
            },
        ),
        (
            8,
            2,
            if x > 0 {
                Some(i - 1)
            } else if c.wrapping {
                Some(y * c.width + c.width - 1)
            } else {
                None
            },
        ),
    ]
}
#[cfg(test)]
fn solution_count(tiles: &[u8], limit: usize) -> usize {
    solution_count_config(
        tiles,
        &vec![0; tiles.len()],
        NetConfig::default(),
        limit,
        &mut 20_000,
    )
}
// Reciprocal ports, hard barriers and connected-tree constraints. Exhaustion is
// conservatively treated as nonunique, so it can never certify a candidate.
fn solution_count_config(
    tiles: &[u8],
    barriers: &[u8],
    c: NetConfig,
    limit: usize,
    budget: &mut usize,
) -> usize {
    let mut domains = Vec::with_capacity(tiles.len());
    for (i, &t) in tiles.iter().enumerate() {
        let mut v = t;
        let mut d = Vec::new();
        for _ in 0..4 {
            if v & barriers[i] == 0
                && !ports(i, c)
                    .iter()
                    .any(|&(a, _, j)| j.is_none() && v & a != 0)
                && !d.contains(&v)
            {
                d.push(v);
            }
            v = rotate(v);
        }
        domains.push(d);
    }
    fn search(
        mut ds: Vec<Vec<u8>>,
        barriers: &[u8],
        c: NetConfig,
        limit: usize,
        budget: &mut usize,
    ) -> usize {
        if *budget == 0 {
            return limit;
        }
        *budget -= 1;
        loop {
            let old = ds.clone();
            for (i, domain) in ds.iter_mut().enumerate() {
                for (a, b, j) in ports(i, c) {
                    if let Some(j) = j {
                        domain.retain(|v| old[j].iter().any(|w| (v & a != 0) == (w & b != 0)));
                    }
                }
                if domain.is_empty() {
                    return 0;
                }
            }
            if old == ds {
                break;
            }
        }
        if let Some(i) = (0..ds.len())
            .filter(|&i| ds[i].len() > 1)
            .min_by_key(|&i| ds[i].len())
        {
            let mut n = 0;
            for v in ds[i].clone() {
                let mut next = ds.clone();
                next[i] = vec![v];
                n += search(next, barriers, c, limit - n, budget);
                if n >= limit {
                    return n;
                }
            }
            n
        } else {
            let tiles = ds.into_iter().map(|d| d[0]).collect();
            let n = Net {
                config: c,
                tiles,
                barriers: barriers.to_vec(),
                locked: Vec::new(),
                solution: Vec::new(),
            };
            usize::from(n.outcome() == Outcome::Won)
        }
    }
    search(domains, barriers, c, limit, budget)
}
impl Net {
    pub fn new(rng: &mut Rng) -> Self {
        // Historical default is always supported. This fallback uses the same
        // settings and a separately tested unique candidate, never extra barriers.
        Self::new_config(rng, NetConfig::default())
            .unwrap_or_else(|_| Self::candidate(&mut Rng(0), NetConfig::default()))
    }
    pub fn new_config(rng: &mut Rng, config: NetConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let mut budget = 100_000;
        for _ in 0..64 {
            let candidate = Self::candidate(rng, config);
            if !config.unique
                || solution_count_config(
                    &candidate.tiles,
                    &candidate.barriers,
                    config,
                    2,
                    &mut budget,
                ) == 1
            {
                return Ok(candidate);
            }
            if budget == 0 {
                break;
            }
        }
        Err(
            "Net uniqueness generation budget exhausted; try another seed or disable unique solution",
        )
    }
    fn candidate(rng: &mut Rng, config: NetConfig) -> Self {
        let n = config.width * config.height;
        let center = (config.height / 2) * config.width + config.width / 2;
        let mut tiles = vec![0; n];
        let mut visited = vec![false; n];
        visited[center] = true;
        let mut stack = vec![center];
        while let Some(&i) = stack.last() {
            let mut candidates: Vec<_> = ports(i, config)
                .into_iter()
                .filter_map(|(a, b, j)| j.filter(|&j| !visited[j]).map(|j| (a, b, j)))
                .collect();
            rng.shuffle(&mut candidates);
            if let Some(&(a, b, j)) = candidates.first() {
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
        // Source net.c:1510-1555 selects a fixed fraction of unused edges
        // after jumbling. A shared shuffled order gives nested barrier sets.
        let mut barriers = vec![0; n];
        let mut choices = Vec::new();
        for i in 0..n {
            for (a, b, j) in ports(i, config) {
                if let Some(j) = j {
                    if i < j && solution[i] & a == 0 {
                        choices.push((i, a, j, b));
                    }
                } else {
                    barriers[i] |= a;
                }
            }
        }
        rng.shuffle(&mut choices);
        let count = choices.len() * config.barrier_percent as usize / 100;
        for &(i, a, j, b) in &choices[..count] {
            barriers[i] |= a;
            barriers[j] |= b;
        }
        let mut s = Self {
            config,
            tiles,
            barriers,
            locked: vec![false; n],
            solution,
        };
        if s.outcome() == Outcome::Won
            && let Some(i) = (0..n).find(|&i| rotate(s.tiles[i]) != s.tiles[i])
        {
            s.tiles[i] = rotate(s.tiles[i]);
        }
        s
    }
    pub(super) fn dimensions(&self) -> (usize, usize) {
        (self.config.width, self.config.height)
    }
    fn connected_at(&self, source: usize) -> Vec<bool> {
        let mut seen = vec![false; self.tiles.len()];
        seen[source] = true;
        let mut q = vec![source];
        while let Some(i) = q.pop() {
            for (a, b, j) in ports(i, self.config) {
                if let Some(j) = j
                    && self.barriers[i] & a == 0
                    && self.barriers[j] & b == 0
                    && self.tiles[i] & a != 0
                    && self.tiles[j] & b != 0
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
        let valid = self.tiles.iter().enumerate().all(|(i, &t)| {
            t & self.barriers[i] == 0
                && ports(i, self.config)
                    .iter()
                    .all(|&(a, b, j)| t & a == 0 || j.is_some_and(|j| self.tiles[j] & b != 0))
        });
        let tree = self
            .tiles
            .iter()
            .map(|t| t.count_ones() as usize)
            .sum::<usize>()
            == 2 * (self.tiles.len() - 1);
        if valid && tree && self.connected_at(0).iter().all(|&v| v) {
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
            width: self.config.width,
            height: self.config.height,
            wrapping: self.config.wrapping,
            barriers: self.barriers.clone(),
            tiles: self.tiles.clone(),
            locked: self.locked.clone(),
            connected: self.connected_at(source),
            source,
        }
    }
    pub(super) fn solve(&mut self) {
        self.tiles = self.solution.clone();
        // Historical net.c:1775 known-answer Solve locks every solved tile.
        self.locked.fill(true);
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        match input {
            PuzzleInput::HalfTurn { cell }
            | PuzzleInput::Rotate { cell, .. }
            | PuzzleInput::Lock { cell }
                if cell >= self.tiles.len() =>
            {
                return Err("Tile outside board");
            }
            PuzzleInput::HalfTurn { cell } => {
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
                if !self.locked[cell] {
                    for _ in 0..if clockwise { 1 } else { 3 } {
                        self.tiles[cell] = rotate(self.tiles[cell]);
                    }
                }
            }
            PuzzleInput::Lock { cell } => self.locked[cell] = !self.locked[cell],
            _ => return Err("Unsupported Net input"),
        }
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

#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn source_stock_sizes_wrap_and_barriers_are_real_rules() {
        for &(width, height) in &[(5, 5), (7, 7), (9, 9), (11, 11), (13, 11)] {
            for wrapping in [false, true] {
                let cfg = NetConfig {
                    width,
                    height,
                    wrapping,
                    barrier_percent: 100,
                    unique: true,
                };
                let mut n = Net::new_config(&mut Rng(77), cfg).unwrap();
                let scene = n.scene_at(width * height / 2);
                assert_eq!(
                    (scene.width, scene.height, scene.wrapping),
                    (width, height, wrapping)
                );
                assert_eq!(scene.barriers.len(), width * height);
                n.tiles = n.solution.clone();
                assert_eq!(n.outcome(), Outcome::Won);
                assert_eq!(
                    solution_count_config(&n.tiles, &n.barriers, cfg, 2, &mut 20_000),
                    1
                );
                for i in 0..width * height {
                    assert_eq!(n.tiles[i] & n.barriers[i], 0);
                }
            }
        }
    }
    #[test]
    fn wrapped_seam_connects_but_barrier_stops_it() {
        let cfg = NetConfig {
            width: 3,
            height: 3,
            wrapping: true,
            barrier_percent: 0,
            unique: false,
        };
        let mut n = Net::new_config(&mut Rng(1), cfg).unwrap();
        n.tiles.fill(0);
        n.barriers.fill(0);
        n.tiles[0] = 8;
        n.tiles[2] = 2;
        assert!(n.connected_at(0)[2]);
        n.barriers[0] |= 8;
        n.barriers[2] |= 2;
        assert!(!n.connected_at(0)[2]);
        n.tiles.fill(0);
        n.barriers.fill(0);
        n.tiles[0] = 1;
        n.tiles[6] = 4;
        assert!(n.connected_at(0)[6]);
        n.barriers[0] = 1;
        n.barriers[6] = 4;
        assert!(!n.connected_at(0)[6]);
    }
}
#[cfg(test)]
mod additional_configuration_tests {
    use super::*;
    #[test]
    fn default_fallback_is_unique_and_loops_do_not_win() {
        let fallback = Net::candidate(&mut Rng(0), NetConfig::default());
        assert_eq!(solution_count(&fallback.tiles, 2), 1);
        let c = NetConfig {
            width: 3,
            height: 3,
            wrapping: false,
            unique: false,
            barrier_percent: 0,
        };
        let mut n = Net::new_config(&mut Rng(2), c).unwrap();
        for i in 0..9 {
            n.tiles[i] = ports(i, c)
                .iter()
                .filter(|(_, _, j)| j.is_some())
                .map(|(a, _, _)| *a)
                .sum();
        }
        assert!(n.connected_at(0).iter().all(|&v| v));
        assert_eq!(n.outcome(), Outcome::Playing);
    }
}
#[cfg(test)]
mod barrier_rate_tests {
    use super::*;
    #[test]
    fn source_barrier_fraction_is_exact_and_rates_are_nested_on_same_candidate() {
        for wrapping in [false, true] {
            let cfg = NetConfig {
                wrapping,
                unique: false,
                barrier_percent: 25,
                ..NetConfig::default()
            };
            let a = Net::new_config(&mut Rng(5), cfg).unwrap();
            let b = Net::new_config(
                &mut Rng(5),
                NetConfig {
                    barrier_percent: 75,
                    ..cfg
                },
            )
            .unwrap();
            assert_eq!(a.tiles, b.tiles);
            assert_eq!(a.solution, b.solution);
            let unused = (0..25)
                .flat_map(|i| {
                    ports(i, cfg)
                        .into_iter()
                        .map(move |(bit, back, j)| (i, bit, back, j))
                })
                .filter(|&(i, bit, _, j)| j.is_some_and(|j| i < j) && a.solution[i] & bit == 0)
                .count();
            let barriers = (0..25)
                .flat_map(|i| {
                    ports(i, cfg)
                        .into_iter()
                        .map(move |(bit, back, j)| (i, bit, back, j))
                })
                .filter(|&(i, bit, _, j)| j.is_some_and(|j| i < j) && a.barriers[i] & bit != 0)
                .count();
            assert_eq!(barriers, unused * 25 / 100);
            for i in 0..25 {
                assert_eq!(a.barriers[i] & !b.barriers[i], 0);
            }
        }
    }
}
