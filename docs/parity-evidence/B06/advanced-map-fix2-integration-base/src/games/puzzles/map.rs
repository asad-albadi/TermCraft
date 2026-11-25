use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapPreset {
    Default,
    Portrait,
}
impl MapPreset {
    pub fn dimensions(self) -> (usize, usize) {
        match self {
            Self::Default => (20, 15),
            Self::Portrait => (16, 18),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum MapDifficulty {
    #[default]
    Easy,
    Normal,
    Hard,
    Unreasonable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapConfig {
    pub preset: MapPreset,
    pub regions: usize,
    pub difficulty: MapDifficulty,
}
impl Default for MapConfig {
    fn default() -> Self {
        Self {
            preset: MapPreset::Default,
            regions: 30,
            difficulty: MapDifficulty::Normal,
        }
    }
}
impl MapConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if ![30, 75].contains(&self.regions) {
            Err("Map supports source presets with 30 or 75 regions")
        } else {
            Ok(())
        }
    }
    pub fn dimensions(self) -> (usize, usize) {
        if self.regions == 30 {
            self.preset.dimensions()
        } else if self.preset == MapPreset::Default {
            (30, 25)
        } else {
            (25, 30)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapGeneration {
    Randomized,
    CertifiedFallback,
}
/// Complete source-tier deductions or bounded unique recursion; no original runtime trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MapCertificate {
    pub single_steps: usize,
    pub pair_eliminations: usize,
    pub requires_normal: bool,
    pub forcing_eliminations: usize,
    pub recursive_nodes: usize,
    pub achieved: MapDifficulty,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapScene {
    pub configuration: MapConfig,
    pub width: usize,
    pub height: usize,
    pub regions: Vec<usize>,
    pub colours: Vec<u8>,
    pub givens: Vec<bool>,
    pub pencil: Vec<u8>,
    pub adjacency: Vec<(usize, usize)>,
    pub conflicts: Vec<bool>,
    pub generation: MapGeneration,
    pub certificate: MapCertificate,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Map {
    config: MapConfig,
    width: usize,
    height: usize,
    regions: Vec<usize>,
    colours: Vec<u8>,
    givens: Vec<bool>,
    pencil: Vec<u8>,
    edges: Vec<(usize, usize)>,
    solution: Vec<u8>,
    generation: MapGeneration,
    certificate: MapCertificate,
}
fn normal_pairs(colours: &[u8], possible: &mut [u8], edges: &[(usize, usize)]) -> usize {
    let mut eliminations = 0;
    for &(a, b) in edges {
        let pair = possible[a];
        if colours[a] == 0 && colours[b] == 0 && pair == possible[b] && pair.count_ones() == 2 {
            for (k, domain) in possible.iter_mut().enumerate() {
                if k == a || k == b {
                    continue;
                }
                let adjacent = |x: usize, y: usize| edges.contains(&(x.min(y), x.max(y)));
                if adjacent(a, k) && adjacent(b, k) && *domain & pair != 0 {
                    *domain &= !pair;
                    eliminations += 1;
                }
            }
        }
    }
    eliminations
}
// Translation of f306b9d map.c:1134–1238 BFS forcing-chain rule.
fn forcing_chains(colours: &[u8], possible: &mut [u8], edges: &[(usize, usize)]) -> usize {
    let n = colours.len();
    let mut eliminated = 0;
    let mut adjacency = vec![Vec::new(); n];
    for &(a, b) in edges {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }
    for start in 0..n {
        if colours[start] != 0 || possible[start].count_ones() != 2 {
            continue;
        }
        for c in 1..=4 {
            let original = 1 << c;
            if possible[start] & original == 0 {
                continue;
            }
            let mut forced = vec![0; n];
            let mut queue = vec![start];
            forced[start] = possible[start] & !original;
            let mut head = 0;
            while head < queue.len() {
                let j = queue[head];
                head += 1;
                let current = forced[j];
                for &k in &adjacency[j] {
                    if forced[k] == 0
                        && colours[k] == 0
                        && possible[k].count_ones() == 2
                        && possible[k] & current != 0
                    {
                        forced[k] = possible[k] & !current;
                        queue.push(k);
                    }
                    if current == original
                        && adjacency[start].contains(&k)
                        && possible[k] & current != 0
                    {
                        possible[k] &= !original;
                        eliminated += 1;
                    }
                }
            }
        }
    }
    eliminated
}
fn grade_solve(
    givens: &[u8],
    edges: &[(usize, usize)],
    grade: MapDifficulty,
) -> Option<(Vec<u8>, MapCertificate)> {
    if grade <= MapDifficulty::Normal {
        let (answer, mut cert) = deduce(givens, edges, grade == MapDifficulty::Normal)?;
        cert.achieved = if deduce(givens, edges, false).is_some() {
            MapDifficulty::Easy
        } else {
            MapDifficulty::Normal
        };
        return Some((answer, cert));
    }
    let (answer, mut cert) = deduce_grade(givens, edges, grade)?;
    cert.achieved = if deduce(givens, edges, false).is_some() {
        MapDifficulty::Easy
    } else if deduce(givens, edges, true).is_some() {
        MapDifficulty::Normal
    } else if grade == MapDifficulty::Hard
        || deduce_grade(givens, edges, MapDifficulty::Hard).is_some()
    {
        MapDifficulty::Hard
    } else {
        MapDifficulty::Unreasonable
    };
    Some((answer, cert))
}
fn deduce_grade(
    givens: &[u8],
    edges: &[(usize, usize)],
    grade: MapDifficulty,
) -> Option<(Vec<u8>, MapCertificate)> {
    let n = givens.len();
    let mut colours = givens.to_vec();
    let mut possible: Vec<u8> = givens
        .iter()
        .map(|&c| if c == 0 { 30 } else { 1 << c })
        .collect();
    let mut cert = MapCertificate::default();
    for _ in 0..=5 * n {
        let mut progress = false;
        for &(a, b) in edges {
            if colours[a] != 0 && colours[a] == colours[b] {
                return None;
            }
            if colours[a] != 0 {
                let old = possible[b];
                possible[b] &= !(1 << colours[a]);
                progress |= old != possible[b];
            }
            if colours[b] != 0 {
                let old = possible[a];
                possible[a] &= !(1 << colours[b]);
                progress |= old != possible[a];
            }
        }
        for i in 0..n {
            if possible[i] == 0 {
                return None;
            }
            if colours[i] == 0 && possible[i].count_ones() == 1 {
                colours[i] = possible[i].trailing_zeros() as u8;
                cert.single_steps += 1;
                progress = true;
            }
        }
        if !colours.contains(&0) {
            return Some((colours, cert));
        }
        if progress {
            continue;
        }
        let changes = normal_pairs(&colours, &mut possible, edges);
        cert.pair_eliminations += changes;
        if changes > 0 {
            continue;
        }
        let changes = forcing_chains(&colours, &mut possible, edges);
        cert.forcing_eliminations += changes;
        if changes > 0 {
            continue;
        }
        break;
    }
    if grade != MapDifficulty::Unreasonable {
        return None;
    }
    // Count to two; exhaustion is never converted into a uniqueness certificate.
    let mut first = None;
    let mut budget = 50_000;
    let count = search(&mut colours.clone(), edges, 2, &mut first, &mut budget);
    if budget == 0 || count != 1 {
        return None;
    }
    cert.recursive_nodes = 50_000 - budget;
    Some((first?, cert))
}
/// Historical map_solver Easy singles and Normal adjacent-pair/common-neighbour elimination.
/// Every step removes a candidate or fills a region: <= 4*n eliminations plus n placements.
fn deduce(
    givens: &[u8],
    edges: &[(usize, usize)],
    normal: bool,
) -> Option<(Vec<u8>, MapCertificate)> {
    let n = givens.len();
    let mut colours = givens.to_vec();
    let mut possible: Vec<u8> = givens
        .iter()
        .map(|&c| if c == 0 { 30 } else { 1 << c })
        .collect();
    let mut certificate = MapCertificate::default();
    for _ in 0..=5 * n {
        let mut progress = false;
        for i in 0..n {
            if colours[i] != 0 {
                let bit = 1 << colours[i];
                for &(a, b) in edges {
                    let other = if a == i {
                        Some(b)
                    } else if b == i {
                        Some(a)
                    } else {
                        None
                    };
                    if let Some(j) = other {
                        if colours[j] == colours[i] {
                            return None;
                        }
                        if possible[j] & bit != 0 {
                            possible[j] &= !bit;
                            progress = true;
                        }
                    }
                }
            }
        }
        for i in 0..n {
            if possible[i] == 0 {
                return None;
            }
            if colours[i] == 0 && possible[i].count_ones() == 1 {
                colours[i] = possible[i].trailing_zeros() as u8;
                certificate.single_steps += 1;
                progress = true;
            }
        }
        if !colours.contains(&0) {
            return Some((colours, certificate));
        }
        if progress {
            continue;
        }
        if normal {
            let eliminations = normal_pairs(&colours, &mut possible, edges);
            certificate.pair_eliminations += eliminations;
            progress = eliminations > 0;
        }
        if !progress {
            return None;
        }
    }
    None
}
fn graph(regions: &[usize], width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for i in 0..regions.len() {
        for j in neighbours(i, width, height, false) {
            let (a, b) = (regions[i].min(regions[j]), regions[i].max(regions[j]));
            if a != b && !edges.contains(&(a, b)) {
                edges.push((a, b));
            }
        }
    }
    edges.sort_unstable();
    edges
}
fn options(c: &[u8], edges: &[(usize, usize)], i: usize) -> u8 {
    let mut bits = 0b11110;
    for &(a, b) in edges {
        if a == i {
            bits &= !(1 << c[b]);
        }
        if b == i {
            bits &= !(1 << c[a]);
        }
    }
    bits
}
fn solve(
    c: &mut [u8],
    edges: &[(usize, usize)],
    limit: usize,
    first: &mut Option<Vec<u8>>,
) -> usize {
    search(c, edges, limit, first, &mut 10_000)
}
fn search(
    c: &mut [u8],
    edges: &[(usize, usize)],
    limit: usize,
    first: &mut Option<Vec<u8>>,
    budget: &mut usize,
) -> usize {
    if *budget == 0 {
        return limit;
    }
    *budget -= 1;
    let best = (0..c.len())
        .filter(|&i| c[i] == 0)
        .map(|i| (i, options(c, edges, i)))
        .min_by_key(|(_, b)| b.count_ones());
    let Some((i, bits)) = best else {
        if first.is_none() {
            *first = Some(c.to_vec());
        }
        return 1;
    };
    let mut n = 0;
    for v in 1..=4 {
        if bits & (1 << v) != 0 {
            c[i] = v;
            n += search(c, edges, limit - n, first, budget);
            c[i] = 0;
            if n >= limit {
                return n;
            }
        }
    }
    n
}
impl Map {
    pub(super) fn solve(&mut self) {
        self.colours = self.solution.clone();
        self.pencil.fill(0);
    }
    const REGIONS: usize = 30;
    pub fn new(rng: &mut Rng) -> Self {
        Self::new_preset(rng, MapPreset::Default)
    }
    pub fn new_preset(rng: &mut Rng, preset: MapPreset) -> Self {
        Self::with_limit(rng, preset, 16)
    }
    fn with_limit(rng: &mut Rng, preset: MapPreset, limit: usize) -> Self {
        Self::generate(rng, preset, limit).unwrap_or_else(|| Self::fallback(preset))
    }
    pub fn new_config(rng: &mut Rng, config: MapConfig) -> Result<Self, &'static str> {
        config.validate()?;
        if config.regions == 30 && config.difficulty == MapDifficulty::Normal {
            return Ok(Self::new_preset(rng, config.preset));
        }
        if let Some(m) = Self::generate_config(rng, config, 16) {
            return Ok(m);
        }
        if config.regions == 30 && config.difficulty == MapDifficulty::Easy {
            let mut m = Self::fallback(config.preset);
            m.config = config;
            for i in 0..m.colours.len() {
                if deduce(&m.colours, &m.edges, false).is_some() {
                    break;
                }
                m.colours[i] = m.solution[i];
            }
            m.givens = m.colours.iter().map(|&c| c != 0).collect();
            m.certificate = deduce(&m.colours, &m.edges, false)
                .expect("complete Easy fallback replay")
                .1;
            return Ok(m);
        }
        Err("Map logical generation budget exhausted; try another seed")
    }
    fn generate(rng: &mut Rng, preset: MapPreset, limit: usize) -> Option<Self> {
        Self::generate_config(
            rng,
            MapConfig {
                preset,
                ..MapConfig::default()
            },
            limit,
        )
    }
    fn generate_config(rng: &mut Rng, config: MapConfig, limit: usize) -> Option<Self> {
        let (width, height) = config.dimensions();
        let cells = width * height;
        let n = config.regions;
        for _ in 0..limit {
            let mut regions = vec![usize::MAX; cells];
            let mut seeds: Vec<_> = (0..cells).collect();
            rng.shuffle(&mut seeds);
            for i in 0..n {
                regions[seeds[i]] = i;
            }
            // At most four directed frontier entries are created per cell.
            // Every pass consumes an entry; stale entries cannot extend the bound.
            let mut frontier = Vec::new();
            for &cell in &seeds[..n] {
                for j in neighbours(cell, width, height, false) {
                    if regions[j] == usize::MAX {
                        frontier.push((j, regions[cell]));
                    }
                }
            }
            for _ in 0..4 * cells {
                if frontier.is_empty() {
                    break;
                }
                let index = rng.below(frontier.len());
                let (cell, region) = frontier.swap_remove(index);
                if regions[cell] != usize::MAX {
                    continue;
                }
                regions[cell] = region;
                for j in neighbours(cell, width, height, false) {
                    if regions[j] == usize::MAX {
                        frontier.push((j, region));
                    }
                }
            }
            if regions.contains(&usize::MAX) {
                continue;
            }
            let edges = graph(&regions, width, height);
            let mut first = None;
            solve(&mut vec![0; n], &edges, 1, &mut first);
            let Some(mut solution) = first else {
                continue;
            };
            for colour in 1..=4 {
                if !solution.contains(&colour) {
                    let mut counts = [0; 5];
                    for &c in &solution {
                        counts[c as usize] += 1;
                    }
                    let i = (0..n).find(|&i| counts[solution[i] as usize] > 1).unwrap();
                    solution[i] = colour;
                }
            }
            let mut colours = solution.clone();
            let mut order: Vec<_> = (0..n).collect();
            rng.shuffle(&mut order);
            for i in order {
                let old = colours[i];
                if colours.iter().filter(|&&c| c == old).count() == 1 {
                    continue;
                }
                colours[i] = 0;
                if grade_solve(&colours, &edges, config.difficulty).is_none() {
                    colours[i] = old;
                }
            }
            if colours.iter().filter(|&&c| c == 0).count() < 6
                || (config.difficulty == MapDifficulty::Normal
                    && deduce(&colours, &edges, false).is_some())
            {
                continue;
            }
            let Some((answer, mut certificate)) = grade_solve(&colours, &edges, config.difficulty)
            else {
                continue;
            };
            if certificate.achieved != config.difficulty || answer != solution {
                continue;
            }
            certificate.requires_normal = config.difficulty >= MapDifficulty::Normal;
            return Some(Self {
                config,
                width,
                height,
                regions,
                colours: colours.clone(),
                givens: colours.iter().map(|&c| c != 0).collect(),
                pencil: vec![0; n],
                edges,
                solution,
                generation: MapGeneration::Randomized,
                certificate,
            });
        }
        None
    }
    fn fallback(preset: MapPreset) -> Self {
        // Locally generated fixtures, not recorded upstream/original boards. Each is
        // independently checked by the Normal deduction certificate in the tests.
        let (regions, colours, solution) = match preset {
            MapPreset::Default => (
                vec![
                    16, 16, 16, 16, 16, 16, 16, 7, 7, 9, 9, 9, 9, 9, 11, 11, 11, 11, 11, 21, 25,
                    16, 16, 16, 16, 16, 16, 7, 7, 22, 15, 15, 9, 9, 9, 11, 11, 11, 11, 21, 16, 16,
                    16, 16, 12, 12, 7, 7, 22, 22, 22, 15, 15, 19, 19, 11, 11, 11, 11, 21, 16, 16,
                    16, 16, 12, 4, 7, 7, 7, 22, 22, 15, 22, 19, 19, 11, 11, 11, 21, 21, 16, 16, 16,
                    12, 12, 4, 7, 7, 7, 22, 22, 22, 22, 22, 19, 11, 11, 13, 13, 21, 0, 0, 0, 0, 12,
                    4, 4, 28, 28, 22, 22, 22, 27, 27, 19, 11, 11, 21, 21, 21, 8, 0, 18, 18, 18, 18,
                    28, 28, 28, 14, 14, 14, 14, 27, 19, 6, 6, 21, 21, 21, 8, 0, 18, 18, 18, 18, 28,
                    28, 28, 14, 14, 14, 14, 6, 6, 6, 6, 21, 21, 21, 8, 0, 0, 18, 18, 18, 28, 28,
                    14, 14, 14, 14, 14, 10, 10, 10, 10, 10, 20, 20, 8, 0, 0, 18, 18, 18, 28, 28,
                    14, 14, 14, 14, 14, 10, 10, 10, 10, 20, 20, 20, 8, 0, 18, 18, 18, 3, 3, 14, 14,
                    14, 14, 14, 14, 23, 23, 23, 23, 20, 24, 20, 26, 26, 18, 3, 3, 3, 3, 14, 14, 14,
                    29, 2, 23, 23, 23, 24, 24, 24, 24, 24, 1, 26, 26, 26, 17, 17, 17, 29, 29, 29,
                    29, 2, 23, 23, 23, 24, 24, 24, 24, 24, 1, 1, 26, 26, 17, 17, 17, 29, 29, 29, 2,
                    2, 2, 2, 23, 23, 24, 5, 5, 5, 1, 1, 26, 26, 17, 17, 17, 29, 29, 29, 2, 2, 2, 2,
                    23, 24, 24, 5, 5, 5,
                ],
                vec![
                    0, 1, 0, 1, 1, 1, 0, 2, 2, 3, 0, 2, 3, 1, 0, 0, 0, 2, 2, 0, 0, 0, 0, 4, 2, 1,
                    3, 0, 3, 3,
                ],
                vec![
                    1, 1, 1, 1, 1, 1, 1, 2, 2, 3, 3, 2, 3, 1, 2, 2, 4, 2, 2, 4, 1, 4, 1, 4, 2, 1,
                    3, 3, 3, 3,
                ],
            ),
            MapPreset::Portrait => (
                vec![
                    26, 26, 26, 26, 26, 13, 13, 13, 13, 4, 4, 4, 4, 4, 4, 4, 26, 26, 26, 26, 13,
                    13, 13, 13, 13, 13, 16, 4, 4, 4, 4, 4, 21, 21, 26, 13, 13, 13, 13, 13, 13, 13,
                    16, 4, 4, 4, 4, 4, 21, 21, 21, 0, 13, 13, 13, 13, 16, 16, 16, 4, 4, 4, 4, 4,
                    21, 21, 0, 0, 13, 13, 13, 18, 16, 16, 4, 4, 4, 4, 4, 4, 23, 21, 0, 0, 5, 5, 5,
                    18, 16, 16, 16, 16, 4, 27, 27, 27, 23, 8, 8, 8, 8, 5, 20, 11, 16, 16, 16, 16,
                    4, 27, 27, 27, 23, 8, 8, 8, 8, 5, 20, 11, 16, 16, 16, 16, 27, 27, 27, 27, 23,
                    8, 8, 9, 9, 9, 16, 16, 16, 2, 16, 27, 27, 27, 27, 27, 23, 23, 8, 9, 9, 9, 16,
                    16, 16, 2, 2, 27, 27, 27, 27, 28, 23, 23, 23, 9, 9, 16, 16, 22, 22, 2, 2, 2,
                    27, 27, 27, 28, 10, 10, 25, 25, 25, 25, 22, 22, 22, 2, 27, 27, 27, 27, 19, 28,
                    10, 10, 12, 25, 25, 25, 22, 22, 6, 2, 27, 27, 19, 19, 19, 28, 1, 12, 12, 25,
                    25, 25, 6, 6, 6, 6, 27, 27, 19, 17, 19, 28, 12, 12, 12, 25, 25, 25, 7, 6, 6, 6,
                    17, 17, 17, 17, 17, 17, 14, 12, 24, 24, 24, 25, 7, 6, 6, 6, 6, 6, 6, 3, 15, 15,
                    14, 12, 24, 24, 24, 25, 7, 6, 6, 6, 6, 6, 15, 15, 15, 15, 14, 12, 12, 12, 24,
                    24, 6, 6, 6, 6, 6, 6, 15, 15, 29, 29,
                ],
                vec![
                    1, 3, 0, 3, 0, 2, 0, 1, 0, 0, 0, 3, 2, 3, 1, 0, 2, 0, 1, 0, 1, 0, 1, 4, 1, 0,
                    1, 0, 3, 1,
                ],
                vec![
                    1, 3, 3, 3, 1, 2, 2, 1, 3, 1, 1, 3, 2, 3, 1, 4, 2, 1, 1, 2, 1, 2, 1, 4, 1, 3,
                    1, 4, 3, 1,
                ],
            ),
        };
        let (width, height) = preset.dimensions();
        let edges = graph(&regions, width, height);
        // These immutable fixtures are part of the source; the complete certificate
        // is regression-tested, and is also recomputed for the public audit metadata.
        let (_, mut certificate) =
            deduce(&colours, &edges, true).expect("certified fallback source invariant");
        certificate.requires_normal = true;
        certificate.achieved = MapDifficulty::Normal;
        Self {
            config: MapConfig {
                preset,
                ..MapConfig::default()
            },
            width,
            height,
            regions,
            givens: colours.iter().map(|&c| c != 0).collect(),
            colours,
            pencil: vec![0; Self::REGIONS],
            edges,
            solution,
            generation: MapGeneration::CertifiedFallback,
            certificate,
        }
    }
    fn conflicts(&self) -> Vec<bool> {
        let mut out = vec![false; self.colours.len()];
        for &(a, b) in &self.edges {
            if self.colours[a] != 0 && self.colours[a] == self.colours[b] {
                out[a] = true;
                out[b] = true;
            }
        }
        out
    }
    pub fn outcome(&self) -> Outcome {
        if !self.colours.contains(&0) && !self.conflicts().contains(&true) {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> MapScene {
        MapScene {
            configuration: self.config,
            width: self.width,
            height: self.height,
            regions: self.regions.clone(),
            colours: self.colours.clone(),
            givens: self.givens.clone(),
            pencil: self.pencil.clone(),
            adjacency: self.edges.clone(),
            conflicts: self.conflicts(),
            generation: self.generation,
            certificate: self.certificate,
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        let (region, colour, pencil) = match input {
            PuzzleInput::Colour { region, colour } => (region, colour, false),
            PuzzleInput::MapPencil { region, colour } => (region, colour, true),
            _ => return Err("Unsupported Map input"),
        };
        if region >= self.colours.len() || colour > 4 {
            return Err("Region/colour outside board");
        }
        if self.givens[region] {
            return Err("Starting region is fixed");
        }
        if pencil {
            if self.colours[region] != 0 {
                return Err("Pencil marks require an empty region");
            }
            if colour == 0 {
                self.pencil[region] = 0;
            } else {
                self.pencil[region] ^= 1 << colour;
            }
        } else {
            self.colours[region] = colour;
            self.pencil[region] = 0;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(mut m: Map) {
        assert_eq!(m.regions.len(), m.width * m.height);
        assert_eq!(m.colours.len(), 30);
        assert!(m.colours.iter().filter(|&&v| v == 0).count() >= 6);
        let (answer, certificate) = deduce(&m.colours, &m.edges, true).unwrap();
        assert_eq!(answer, m.solution);
        assert!(certificate.pair_eliminations > 0);
        assert!(deduce(&m.colours, &m.edges, false).is_none());
        assert_eq!(solve(&mut m.colours.clone(), &m.edges, 2, &mut None), 1);
        for colour in 1..=4 {
            assert!(m.colours.contains(&colour));
        }
        for region in 0..m.colours.len() {
            let cells: Vec<_> = (0..m.regions.len())
                .filter(|&i| m.regions[i] == region)
                .collect();
            assert!(!cells.is_empty());
            let mut seen = vec![cells[0]];
            let mut q = seen.clone();
            while let Some(i) = q.pop() {
                for j in neighbours(i, m.width, m.height, false) {
                    if m.regions[j] == region && !seen.contains(&j) {
                        seen.push(j);
                        q.push(j);
                    }
                }
            }
            assert_eq!(seen.len(), cells.len());
            if !m.givens[region] {
                m.apply(PuzzleInput::Colour {
                    region,
                    colour: m.solution[region],
                })
                .unwrap();
            }
        }
        assert_eq!(m.outcome(), Outcome::Won);
    }
    #[test]
    fn recovered_default_normal_deductions_and_connected_regions() {
        for preset in [MapPreset::Default, MapPreset::Portrait] {
            let mut layouts = Vec::new();
            let mut randomized = 0;
            for seed in 0..30 {
                let m = Map::new_preset(&mut Rng(seed), preset);
                assert_eq!((m.width, m.height), preset.dimensions());
                if m.generation == MapGeneration::Randomized {
                    randomized += 1;
                }
                if !layouts.contains(&m.regions) {
                    layouts.push(m.regions.clone());
                }
                check(m);
            }
            println!(
                "{preset:?}: {randomized}/30 randomized, {} distinct layouts",
                layouts.len()
            );
            assert!(randomized >= 20);
            assert!(layouts.len() >= 20);
        }
    }
    #[test]
    fn exhaustion_exposes_certified_normal_fallback() {
        for preset in [MapPreset::Default, MapPreset::Portrait] {
            let m = Map::with_limit(&mut Rng(0), preset, 0);
            assert_eq!(m.generation, MapGeneration::CertifiedFallback);
            assert!(m.certificate.requires_normal);
            assert!(m.certificate.pair_eliminations > 0);
            check(m);
        }
    }
    #[test]
    fn portrait_reset_preserves_dimensions() {
        let mut g = PuzzleSession::new_map(0, MapPreset::Portrait);
        g.reset();
        let PuzzleScene::Map(s) = g.scene() else {
            panic!()
        };
        assert_eq!((s.width, s.height, s.colours.len()), (16, 18, 30));
    }
}

#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d puzzles.but map: edge neighbours differ; diagonal-only contact may match.
    #[test]
    fn recovered_border_and_colour_transitions() {
        // Four quadrants: 0 1 / 2 3. Only orthogonal borders form adjacency.
        let edges = vec![(0, 1), (0, 2), (1, 3), (2, 3)];
        let mut m = Map {
            config: MapConfig::default(),
            width: 2,
            height: 2,
            generation: MapGeneration::Randomized,
            certificate: MapCertificate::default(),
            regions: Vec::new(),
            colours: vec![0; 16],
            givens: vec![false; 16],
            pencil: vec![0; 16],
            edges,
            solution: Vec::new(),
        };
        m.apply(PuzzleInput::Colour {
            region: 0,
            colour: 1,
        })
        .unwrap();
        m.apply(PuzzleInput::Colour {
            region: 3,
            colour: 1,
        })
        .unwrap();
        assert!(!m.conflicts()[0]);
        m.apply(PuzzleInput::Colour {
            region: 1,
            colour: 1,
        })
        .unwrap();
        assert!(m.conflicts()[0]);
        m.apply(PuzzleInput::Colour {
            region: 1,
            colour: 0,
        })
        .unwrap();
        assert!(!m.conflicts()[0]);
        m.apply(PuzzleInput::MapPencil {
            region: 1,
            colour: 4,
        })
        .unwrap();
        assert_eq!(m.pencil[1], 16);
        assert_eq!(m.colours[1], 0);
        m.givens[0] = true;
        assert!(
            m.apply(PuzzleInput::Colour {
                region: 0,
                colour: 2
            })
            .is_err()
        );
        assert!(
            m.apply(PuzzleInput::Colour {
                region: 1,
                colour: 5
            })
            .is_err()
        );
    }
}
#[cfg(test)]
mod normal_source_fixtures {
    use super::*;
    // map.c1033–1098: an adjacent undecided pair with equal two-colour domains
    // excludes BOTH colours from every shared neighbour, not other regions.
    #[test]
    fn recovered_normal_adjacent_pair_elimination() {
        let edges = [(0, 1), (0, 2), (1, 2), (0, 3)];
        let mut possible = [6, 6, 30, 30];
        assert_eq!(normal_pairs(&[0; 4], &mut possible, &edges), 1);
        assert_eq!(possible, [6, 6, 24, 30]);
        let mut possible = [6, 10, 30, 30];
        assert_eq!(normal_pairs(&[0; 4], &mut possible, &edges), 0);
        assert_eq!(possible, [6, 10, 30, 30]);
    }
}
#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn easy_presets_use_only_source_easy_singles() {
        for preset in [MapPreset::Default, MapPreset::Portrait] {
            let cfg = MapConfig {
                preset,
                regions: 30,
                difficulty: MapDifficulty::Easy,
            };
            let m = Map::new_config(&mut Rng(2), cfg).unwrap();
            let (answer, certificate) = deduce(&m.colours, &m.edges, false).unwrap();
            assert_eq!(answer, m.solution);
            assert_eq!(certificate.pair_eliminations, 0);
            assert!(!m.scene().certificate.requires_normal);
            assert!(m.colours.contains(&0));
        }
    }
    #[test]
    fn large_normal_presets_have_seventy_five_connected_regions() {
        for preset in [MapPreset::Default, MapPreset::Portrait] {
            let cfg = MapConfig {
                preset,
                regions: 75,
                difficulty: MapDifficulty::Normal,
            };
            let m = Map::new_config(&mut Rng(2), cfg).unwrap();
            assert_eq!(m.colours.len(), 75);
            assert_eq!(
                (m.width, m.height),
                if preset == MapPreset::Default {
                    (30, 25)
                } else {
                    (25, 30)
                }
            );
            assert!(deduce(&m.colours, &m.edges, false).is_none());
            assert_eq!(deduce(&m.colours, &m.edges, true).unwrap().0, m.solution);
            for region in 0..75 {
                let first = m.regions.iter().position(|&r| r == region).unwrap();
                let mut reached = vec![false; m.regions.len()];
                let mut q = vec![first];
                reached[first] = true;
                while let Some(i) = q.pop() {
                    for j in neighbours(i, m.width, m.height, false) {
                        if m.regions[j] == region && !reached[j] {
                            reached[j] = true;
                            q.push(j);
                        }
                    }
                }
                assert_eq!(
                    reached.iter().filter(|&&v| v).count(),
                    m.regions.iter().filter(|&&r| r == region).count()
                );
            }
        }
    }
}

#[cfg(test)]
mod advanced_tests {
    use super::*;
    #[test]
    fn forcing_chain_deduction_is_real() {
        // a:12 -- b:23 -- c:13 forces at least one endpoint to colour 1;
        // common neighbour d cannot be 1 (Normal pair rule cannot see this).
        let edges = vec![(0, 1), (0, 3), (1, 2), (2, 3)];
        let colours = vec![0; 4];
        let mut p = vec![6, 12, 10, 30];
        assert_eq!(normal_pairs(&colours, &mut p, &edges), 0);
        assert!(forcing_chains(&colours, &mut p, &edges) > 0);
        assert_eq!(p[3] & 2, 0);
    }
    #[test]
    fn advanced_map_stock_grades_are_exact_or_explicitly_exhausted() {
        for difficulty in [MapDifficulty::Hard, MapDifficulty::Unreasonable] {
            let c = MapConfig {
                difficulty,
                ..MapConfig::default()
            };
            let mut successes = 0;
            for seed in 0..12 {
                if let Ok(m) = Map::new_config(&mut Rng(seed), c) {
                    assert_eq!(m.certificate.achieved, difficulty);
                    assert!(deduce(&m.colours, &m.edges, true).is_none());
                    if difficulty == MapDifficulty::Unreasonable {
                        assert!(grade_solve(&m.colours, &m.edges, MapDifficulty::Hard).is_none());
                    }
                    let mut first = None;
                    assert_eq!(solve(&mut m.colours.clone(), &m.edges, 2, &mut first), 1);
                    successes += 1;
                }
            }
            assert!(successes > 0, "{difficulty:?} produced no boards");
        }
    }
}
#[cfg(test)]
mod advanced_stock_matrix {
    use super::*;
    fn independent_count(
        colours: &mut [u8],
        edges: &[(usize, usize)],
        budget: &mut usize,
    ) -> usize {
        assert!(*budget > 0);
        *budget -= 1;
        let best = colours
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == 0)
            .map(|(i, _)| {
                let values: Vec<_> = (1..=4)
                    .filter(|&v| {
                        edges.iter().all(|&(a, b)| {
                            if a == i {
                                colours[b] != v
                            } else if b == i {
                                colours[a] != v
                            } else {
                                true
                            }
                        })
                    })
                    .collect();
                (i, values)
            })
            .min_by_key(|(_, v)| v.len());
        let Some((i, values)) = best else {
            return 1;
        };
        let mut total = 0;
        for v in values {
            colours[i] = v;
            total += independent_count(colours, edges, budget);
            colours[i] = 0;
            if total >= 2 {
                break;
            }
        }
        total
    }
    #[test]
    fn all_six_remaining_map_presets_have_exact_grade_samples() {
        for preset in [MapPreset::Default, MapPreset::Portrait] {
            for (regions, difficulty) in [
                (30, MapDifficulty::Hard),
                (30, MapDifficulty::Unreasonable),
                (75, MapDifficulty::Hard),
            ] {
                let cfg = MapConfig {
                    preset,
                    regions,
                    difficulty,
                };
                let mut successes = 0;
                for seed in 0..12 {
                    match Map::new_config(&mut Rng(seed), cfg) {
                        Ok(m) => {
                            assert_eq!(m.certificate.achieved, difficulty);
                            let mut independent_edges = std::collections::BTreeSet::new();
                            for y in 0..m.height {
                                for x in 0..m.width {
                                    let i = y * m.width + x;
                                    for j in [
                                        ((x + 1) < m.width).then_some(i + 1),
                                        ((y + 1) < m.height).then_some(i + m.width),
                                    ]
                                    .into_iter()
                                    .flatten()
                                    {
                                        let (a, b) = (m.regions[i], m.regions[j]);
                                        if a != b {
                                            independent_edges.insert((a.min(b), a.max(b)));
                                        }
                                    }
                                }
                            }
                            let edges: Vec<_> = independent_edges.into_iter().collect();
                            assert_eq!(m.edges, edges);
                            assert_eq!(
                                independent_count(&mut m.colours.clone(), &edges, &mut 50_000),
                                1
                            );
                            assert!(
                                grade_solve(
                                    &m.colours,
                                    &m.edges,
                                    if difficulty == MapDifficulty::Hard {
                                        MapDifficulty::Normal
                                    } else {
                                        MapDifficulty::Hard
                                    }
                                )
                                .is_none()
                            );
                            for region in 0..regions {
                                let mut reached =
                                    vec![m.regions.iter().position(|&r| r == region).unwrap()];
                                let mut head = 0;
                                while head < reached.len() {
                                    let i = reached[head];
                                    head += 1;
                                    for j in neighbours(i, m.width, m.height, false) {
                                        if m.regions[j] == region && !reached.contains(&j) {
                                            reached.push(j);
                                        }
                                    }
                                }
                                assert_eq!(
                                    reached.len(),
                                    m.regions.iter().filter(|&&r| r == region).count()
                                );
                            }
                            eprintln!(
                                "Map {cfg:?}: seed {seed}, forcing {}, recursive {}",
                                m.certificate.forcing_eliminations, m.certificate.recursive_nodes
                            );
                            let mut session =
                                PuzzleSession::new_config(seed, PuzzleConfig::Map(cfg)).unwrap();
                            let initial = session.scene();
                            session.solve().unwrap();
                            assert_eq!(session.outcome(), Outcome::Won);
                            assert!(session.assisted());
                            let solved = session.scene();
                            assert!(session.undo());
                            assert_eq!(session.scene(), initial);
                            assert!(!session.assisted());
                            assert!(session.redo());
                            assert_eq!(session.scene(), solved);
                            session.reset();
                            assert_eq!(session.scene(), initial);
                            assert!(!session.assisted());
                            assert!(!session.can_undo());
                            assert!(!session.can_redo());
                            successes += 1;
                            break;
                        }
                        Err(e) => assert!(e.contains("budget exhausted")),
                    }
                }
                assert!(successes > 0, "no exact-grade sample for {cfg:?}");
            }
        }
    }
}
