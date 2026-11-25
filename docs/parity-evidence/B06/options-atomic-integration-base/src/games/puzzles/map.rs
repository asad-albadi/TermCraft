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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapGeneration {
    Randomized,
    CertifiedFallback,
}
/// Counts from a complete no-guess deduction replay, not a claimed original runtime trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MapCertificate {
    pub single_steps: usize,
    pub pair_eliminations: usize,
    pub requires_normal: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapScene {
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
    width: usize,
    height: usize,
    preset: MapPreset,
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
    fn generate(rng: &mut Rng, preset: MapPreset, limit: usize) -> Option<Self> {
        let (width, height) = preset.dimensions();
        let cells = width * height;
        let n = Self::REGIONS;
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
                if deduce(&colours, &edges, true).is_none() {
                    colours[i] = old;
                }
            }
            if colours.iter().filter(|&&c| c == 0).count() < 6
                || deduce(&colours, &edges, false).is_some()
            {
                continue;
            }
            let Some((answer, mut certificate)) = deduce(&colours, &edges, true) else {
                continue;
            };
            if answer != solution {
                continue;
            }
            certificate.requires_normal = true;
            return Some(Self {
                width,
                height,
                preset,
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
        Self {
            width,
            height,
            preset,
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
    pub fn preset(&self) -> MapPreset {
        self.preset
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
            width: 2,
            height: 2,
            preset: MapPreset::Default,
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
