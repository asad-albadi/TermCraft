//! Source-derived safe Rust deduction rules from Simon Tatham's Puzzles,
//! f306b9db55453f7d7fc824bbbf89ca6ac7d2d273 (20150612 historical source).
//! Copyright (c) 2004-2014 Simon Tatham and contributors; MIT-style licence
//! retained verbatim at provenance/UPSTREAM-LICENCE. Generation is independent.
//! See PROVENANCE.md and provenance/advanced-source-manifest.json.
use super::*;
pub(super) fn box_regions(c: SoloConfig) -> Vec<usize> {
    let n = c.block_width * c.block_height;
    (0..n * n)
        .map(|i| i / n / c.block_height * (n / c.block_width) + i % n / c.block_width)
        .collect()
}
pub(super) fn rule_peers(
    i: usize,
    j: usize,
    c: SoloConfig,
    regions: &[usize],
    cages: &[SoloCage],
) -> bool {
    let n = c.block_width * c.block_height;
    i / n == j / n
        || i % n == j % n
        || regions[i] == regions[j]
        || (c.diagonal
            && ((i / n == i % n && j / n == j % n)
                || (i / n + i % n == n - 1 && j / n + j % n == n - 1)))
        || cages
            .iter()
            .any(|k| k.cells.contains(&i) && k.cells.contains(&j))
}
#[derive(Clone)]
struct Rules {
    c: SoloConfig,
    cages: Vec<SoloCage>,
    units: Vec<Vec<usize>>,
    peers: Vec<Vec<usize>>,
}
impl Rules {
    fn new(c: SoloConfig, regions: Vec<usize>, cages: Vec<SoloCage>) -> Self {
        let n = c.block_width * c.block_height;
        let mut units: Vec<Vec<usize>> = (0..n)
            .map(|r| (0..n * n).filter(|&i| regions[i] == r).collect())
            .collect();
        for y in 0..n {
            units.push((0..n).map(|x| y * n + x).collect());
        }
        for x in 0..n {
            units.push((0..n).map(|y| y * n + x).collect());
        }
        if c.diagonal {
            units.push((0..n).map(|i| i * n + i).collect());
            units.push((0..n).map(|i| i * n + n - 1 - i).collect());
        }
        let peers = (0..n * n)
            .map(|i| {
                (0..n * n)
                    .filter(|&j| j != i && rule_peers(i, j, c, &regions, &cages))
                    .collect()
            })
            .collect();
        Self {
            c,
            cages,
            units,
            peers,
        }
    }
    fn valid(&self, d: &[u8]) -> bool {
        for (i, &v) in d.iter().enumerate() {
            if v != 0 && self.peers[i].iter().any(|&j| d[j] == v) {
                return false;
            }
        }
        for k in &self.cages {
            let sum: u16 = k.cells.iter().map(|&i| d[i] as u16).sum();
            let empty = k.cells.iter().filter(|&&i| d[i] == 0).count();
            if sum > k.sum || (empty == 0 && sum != k.sum) {
                return false;
            }
        }
        true
    }
    fn possibilities(&self, d: &[u8], i: usize) -> u32 {
        let n = self.c.block_width * self.c.block_height;
        let mut bits = ((1u32 << (n + 1)) - 1) & !1;
        for &j in &self.peers[i] {
            bits &= !(1 << d[j]);
        }
        for k in self.cages.iter().filter(|k| k.cells.contains(&i)) {
            let sum: usize = k
                .cells
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| d[j] as usize)
                .sum();
            let empty = k.cells.iter().filter(|&&j| j != i && d[j] == 0).count();
            for v in 1..=n {
                if sum + v + empty > k.sum as usize || sum + v + empty * n < (k.sum as usize) {
                    bits &= !(1 << v);
                }
            }
        }
        bits
    }
}
fn certificate(grade: SoloDifficulty) -> SoloCertificate {
    SoloCertificate {
        requested: grade,
        achieved: SoloDifficulty::Trivial,
        easier_fallback: false,
        block_steps: 0,
        basic_steps: 0,
        intersection_eliminations: 0,
        set_eliminations: 0,
        extreme_eliminations: 0,
        recursive_nodes: 0,
        cage_eliminations: 0,
        killer_intersection_required: false,
    }
}
fn intersection(p: &mut [u32], units: &[Vec<usize>], n: usize) -> usize {
    // Source compares rows/columns/diagonals with blocks, in both directions.
    for b in 0..n {
        for line in n..units.len() {
            for (a, z) in [(b, line), (line, b)] {
                for digit in 1..=n {
                    let bit = 1 << digit;
                    let choices: Vec<_> = units[a]
                        .iter()
                        .copied()
                        .filter(|&i| p[i] & bit != 0)
                        .collect();
                    if !choices.is_empty() && choices.iter().all(|i| units[z].contains(i)) {
                        let mut changes = 0;
                        for &i in &units[z] {
                            if !units[a].contains(&i) && p[i] & bit != 0 {
                                p[i] &= !bit;
                                changes += 1;
                            }
                        }
                        if changes > 0 {
                            return changes;
                        }
                    }
                }
            }
        }
    }
    0
}
// The source's zero-rectangle set rule is equivalent to Hall subsets of
// unresolved cells (and its complement supplies hidden subsets).
fn set_eliminate(p: &mut [u32], unit: &[usize]) -> usize {
    let cells: Vec<_> = unit.iter().copied().filter(|&i| p[i] != 0).collect();
    let n = cells.len();
    if n < 4 {
        return 0;
    }
    for set in 1u32..(1 << n) {
        let size = set.count_ones() as usize;
        if size < 2 || size >= n - 1 {
            continue;
        }
        let mut union = 0;
        for (k, &i) in cells.iter().enumerate() {
            if set & (1 << k) != 0 {
                union |= p[i];
            }
        }
        if union.count_ones() as usize != size {
            continue;
        }
        let mut changes = 0;
        for (k, &i) in cells.iter().enumerate() {
            if set & (1 << k) == 0 {
                let old = p[i];
                p[i] &= !union;
                changes += (old ^ p[i]).count_ones() as usize;
            }
        }
        if changes > 0 {
            return changes;
        }
    }
    0
}
fn fish(p: &mut [u32], n: usize) -> usize {
    for digit in 1..=n {
        let bit = 1 << digit;
        let rows: Vec<_> = (0..n)
            .filter(|&y| (0..n).any(|x| p[y * n + x] & bit != 0))
            .collect();
        for set in 1u32..(1 << rows.len()) {
            let size = set.count_ones() as usize;
            if size < 2 || size >= rows.len() - 1 {
                continue;
            }
            let mut cols = 0u32;
            for (k, &y) in rows.iter().enumerate() {
                if set & (1 << k) != 0 {
                    for x in 0..n {
                        if p[y * n + x] & bit != 0 {
                            cols |= 1 << x;
                        }
                    }
                }
            }
            if cols.count_ones() as usize != size {
                continue;
            }
            let mut changes = 0;
            for (k, &y) in rows.iter().enumerate() {
                if set & (1 << k) == 0 {
                    for x in 0..n {
                        if cols & (1 << x) != 0 && p[y * n + x] & bit != 0 {
                            p[y * n + x] &= !bit;
                            changes += 1;
                        }
                    }
                }
            }
            if changes > 0 {
                return changes;
            }
        }
    }
    0
}
fn forcing(p: &mut [u32], r: &Rules) -> usize {
    for start in 0..p.len() {
        if p[start].count_ones() != 2 {
            continue;
        }
        for v in 1..=9 {
            let orig = 1 << v;
            if p[start] & orig == 0 {
                continue;
            }
            let mut forced = vec![0; p.len()];
            forced[start] = p[start] & !orig;
            let mut queue = vec![start];
            let mut head = 0;
            while head < queue.len() {
                let j = queue[head];
                head += 1;
                let current = forced[j];
                for &k in &r.peers[j] {
                    if forced[k] != 0 || p[k] & current == 0 {
                        continue;
                    }
                    if p[k].count_ones() == 2 {
                        forced[k] = p[k] & !current;
                        queue.push(k);
                    }
                    if current == orig && r.peers[start].contains(&k) {
                        p[k] &= !orig;
                        return 1;
                    }
                }
            }
        }
    }
    0
}
fn reduced_cages(d: &[u8], cages: &[SoloCage]) -> Option<Vec<SoloCage>> {
    let mut result = Vec::new();
    for k in cages {
        let filled: u16 = k.cells.iter().map(|&i| d[i] as u16).sum();
        if filled > k.sum {
            return None;
        }
        let cells: Vec<_> = k.cells.iter().copied().filter(|&i| d[i] == 0).collect();
        if cells.is_empty() {
            if filled != k.sum {
                return None;
            }
        } else {
            result.push(SoloCage {
                cells,
                sum: k.sum - filled,
            });
        }
    }
    Some(result)
}
fn sum_masks(size: usize, sum: usize) -> &'static [u32] {
    static TABLE: std::sync::OnceLock<Vec<Vec<Vec<u32>>>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = vec![vec![Vec::new(); 46]; 5];
        for bits in 2u32..1024 {
            let count = bits.count_ones() as usize;
            if bits & 1 == 0 && (2..=4).contains(&count) {
                let sum: usize = (1..=9).filter(|v| bits & (1 << v) != 0).sum();
                t[count][sum].push(bits);
            }
        }
        t
    });
    if sum > 45 {
        return &[];
    }
    &table[size][sum]
}
fn killer_constraints(
    d: &[u8],
    p: &mut [u32],
    r: &Rules,
    enable_intersections: bool,
) -> Option<(Vec<SoloCage>, usize)> {
    let mut cages = reduced_cages(d, &r.cages)?;
    let mut extra = Vec::new();
    // DIFF_KINTERSECT: 45 minus whole cages/filled cells derives residual cages;
    // if contained in one cage, split that cage and derive its complement too.
    if enable_intersections {
        for unit in r.units.iter().take(27) {
            let mut cells: Vec<_> = unit.iter().copied().filter(|&i| d[i] == 0).collect();
            let mut sum = 45i32 - unit.iter().map(|&i| d[i] as i32).sum::<i32>();
            let before = cells.len();
            for k in &cages {
                if k.cells.iter().all(|i| cells.contains(i)) {
                    sum -= k.sum as i32;
                    cells.retain(|i| !k.cells.contains(i));
                }
            }
            if cells.is_empty() {
                if sum != 0 {
                    return None;
                }
                continue;
            }
            if cells.len() == before {
                continue;
            }
            if sum <= 0 {
                return None;
            }
            let residual = SoloCage {
                cells: cells.clone(),
                sum: sum as u16,
            };
            if let Some(b) = cages.iter().position(|k| {
                cells.iter().all(|i| k.cells.contains(i)) && k.cells.len() > cells.len()
            }) {
                if cages[b].sum < residual.sum {
                    return None;
                }
                cages[b].sum -= residual.sum;
                cages[b].cells.retain(|i| !cells.contains(i));
                cages.push(residual);
            } else {
                extra.push(residual);
            }
        }
    }
    let mut changes = 0;
    for k in cages.iter().chain(extra.iter()) {
        if k.cells.len() == 1 {
            continue;
        }
        // DIFF_KMINMAX uses independent extrema, without all-different matching.
        for &i in &k.cells {
            for v in 1..=9 {
                let bit = 1 << v;
                if p[i] & bit == 0 {
                    continue;
                }
                let mut lo = v;
                let mut hi = v;
                for &j in &k.cells {
                    if i != j {
                        if p[j] == 0 {
                            return None;
                        }
                        lo += p[j].trailing_zeros();
                        hi += 31 - p[j].leading_zeros();
                    }
                }
                if lo > k.sum as u32 || hi < (k.sum as u32) {
                    p[i] &= !bit;
                    changes += 1;
                }
            }
        }
        // DIFF_KSUMS: source union of distinct 2/3/4-number sum sets,
        // retaining a set when each cell has at least one member.
        let region = cages.contains(k)
            || r.units
                .iter()
                .take(27)
                .any(|u| k.cells.iter().all(|i| u.contains(i)));
        if region && (2..=4).contains(&k.cells.len()) {
            let mut union = 0;
            for &bits in sum_masks(k.cells.len(), k.sum as usize) {
                if k.cells.iter().all(|&i| p[i] & bits != 0) {
                    union |= bits;
                }
            }
            if union == 0 {
                return None;
            }
            for &i in &k.cells {
                let old = p[i];
                p[i] &= union;
                changes += (old ^ p[i]).count_ones() as usize;
            }
        }
    }
    cages.extend(extra);
    Some((cages, changes))
}
fn logical(input: &[u8], r: &Rules, max: SoloDifficulty) -> Option<(Vec<u8>, SoloCertificate)> {
    logical_with_killer(input, r, max, true)
}
fn logical_with_killer(
    input: &[u8],
    r: &Rules,
    max: SoloDifficulty,
    enable_intersections: bool,
) -> Option<(Vec<u8>, SoloCertificate)> {
    if !r.valid(input) {
        return None;
    }
    let n = r.c.block_width * r.c.block_height;
    let mut d = input.to_vec();
    let mut p = vec![0; d.len()];
    for i in 0..d.len() {
        if d[i] == 0 {
            p[i] = r.possibilities(&d, i);
        }
    }
    let mut cert = certificate(max);
    for _ in 0..=n * n * (n + 1) {
        if !d.contains(&0) {
            return r.valid(&d).then_some((d, cert));
        }
        for i in 0..d.len() {
            if d[i] == 0 {
                p[i] &= r.possibilities(&d, i);
                if p[i] == 0 {
                    return None;
                }
            } else {
                p[i] = 0;
            }
        }
        let mut step = None;
        let limit = if max == SoloDifficulty::Trivial {
            n
        } else {
            r.units.len()
        };
        'singles: for (index, unit) in r.units.iter().take(limit).enumerate() {
            for digit in 1..=n {
                if unit.iter().any(|&i| d[i] == digit as u8) {
                    continue;
                }
                let choices: Vec<_> = unit
                    .iter()
                    .copied()
                    .filter(|&i| p[i] & (1 << digit) != 0)
                    .collect();
                if choices.is_empty() {
                    return None;
                }
                if choices.len() == 1 {
                    step = Some((choices[0], digit as u8, index < n));
                    break 'singles;
                }
            }
        }
        if let Some((i, v, block)) = step {
            d[i] = v;
            if block {
                cert.block_steps += 1;
            } else {
                cert.basic_steps += 1;
                cert.achieved = cert.achieved.max(SoloDifficulty::Basic);
            }
            continue;
        }
        if !r.cages.is_empty() {
            let (cages, changes) = killer_constraints(&d, &mut p, r, enable_intersections)?;
            if let Some(k) = cages.iter().find(|k| k.cells.len() == 1) {
                let i = k.cells[0];
                if k.sum > 9 || k.sum == 0 || p[i] & (1 << k.sum) == 0 {
                    return None;
                }
                d[i] = k.sum as u8;
                cert.cage_eliminations += 1;
                continue;
            }
            if changes > 0 {
                cert.cage_eliminations += changes;
                continue;
            }
        }
        if max == SoloDifficulty::Trivial {
            return None;
        }
        if let Some(i) = (0..p.len()).find(|&i| d[i] == 0 && p[i].count_ones() == 1) {
            d[i] = p[i].trailing_zeros() as u8;
            cert.basic_steps += 1;
            cert.achieved = cert.achieved.max(SoloDifficulty::Basic);
            continue;
        }
        if max == SoloDifficulty::Basic {
            return None;
        }
        let changes = intersection(&mut p, &r.units, n);
        if changes > 0 {
            cert.intersection_eliminations += changes;
            cert.achieved = cert.achieved.max(SoloDifficulty::Intermediate);
            continue;
        }
        if max == SoloDifficulty::Intermediate {
            return None;
        }
        let changes = r
            .units
            .iter()
            .find_map(|u| {
                let c = set_eliminate(&mut p, u);
                (c > 0).then_some(c)
            })
            .unwrap_or(0);
        if changes > 0 {
            cert.set_eliminations += changes;
            cert.achieved = cert.achieved.max(SoloDifficulty::Advanced);
            continue;
        }
        if max == SoloDifficulty::Advanced {
            return None;
        }
        let mut changes = fish(&mut p, n);
        if changes == 0 {
            changes = forcing(&mut p, r);
        }
        if changes > 0 {
            cert.extreme_eliminations += changes;
            cert.achieved = cert.achieved.max(SoloDifficulty::Extreme);
            continue;
        }
        if max != SoloDifficulty::Unreasonable {
            return None;
        }
        let mut first = None;
        let mut budget = 100_000;
        let count = solution_count(&mut d.clone(), r, 2, &mut budget, &mut first);
        if budget == 0 || count != 1 {
            return None;
        }
        cert.recursive_nodes = 100_000 - budget;
        cert.achieved = SoloDifficulty::Unreasonable;
        return Some((first?, cert));
    }
    None
}
fn solution_count(
    d: &mut [u8],
    r: &Rules,
    limit: usize,
    budget: &mut usize,
    first: &mut Option<Vec<u8>>,
) -> usize {
    if *budget == 0 {
        return limit;
    }
    *budget -= 1;
    let best = (0..d.len())
        .filter(|&i| d[i] == 0)
        .map(|i| (i, r.possibilities(d, i)))
        .min_by_key(|(_, bits)| bits.count_ones());
    let Some((i, bits)) = best else {
        if !r.valid(d) {
            return 0;
        }
        if first.is_none() {
            *first = Some(d.to_vec());
        }
        return 1;
    };
    let mut found = 0;
    for v in 1..=r.c.block_width * r.c.block_height {
        if bits & (1 << v) != 0 {
            d[i] = v as u8;
            found += solution_count(d, r, limit - found, budget, first);
            d[i] = 0;
            if found >= limit {
                return found;
            }
        }
    }
    found
}
fn random_solution(d: &mut [u8], r: &Rules, rng: &mut Rng, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let best = (0..d.len())
        .filter(|&i| d[i] == 0)
        .map(|i| (i, r.possibilities(d, i)))
        .min_by_key(|(_, bits)| bits.count_ones());
    let Some((i, bits)) = best else {
        return r.valid(d);
    };
    let mut values: Vec<_> = (1..=9).filter(|v| bits & (1 << v) != 0).collect();
    rng.shuffle(&mut values);
    for v in values {
        d[i] = v;
        if random_solution(d, r, rng, budget) {
            return true;
        }
    }
    d[i] = 0;
    false
}
fn connected(regions: &[usize], id: usize) -> bool {
    let cells: Vec<_> = (0..regions.len()).filter(|&i| regions[i] == id).collect();
    if cells.is_empty() {
        return false;
    }
    let mut reached = vec![cells[0]];
    let mut head = 0;
    while head < reached.len() {
        let i = reached[head];
        head += 1;
        for j in neighbours(i, 9, 9, false) {
            if regions[j] == id && !reached.contains(&j) {
                reached.push(j);
            }
        }
    }
    reached.len() == cells.len()
}
fn jigsaw(regions: &mut [usize], solution: &[u8], rng: &mut Rng) -> bool {
    let old = regions.to_vec();
    for _ in 0..2000 {
        let i = rng.below(81);
        let candidates: Vec<_> = (0..81)
            .filter(|&j| {
                regions[j] != regions[i]
                    && solution[i] == solution[j]
                    && neighbours(i, 9, 9, false)
                        .iter()
                        .any(|&k| regions[k] == regions[j])
                    && neighbours(j, 9, 9, false)
                        .iter()
                        .any(|&k| regions[k] == regions[i])
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let j = candidates[rng.below(candidates.len())];
        let (a, b) = (regions[i], regions[j]);
        regions.swap(i, j);
        if !connected(regions, a) || !connected(regions, b) {
            regions.swap(i, j);
        }
    }
    regions != old
}
fn killer_cages(
    solution: &[u8],
    rng: &mut Rng,
    _c: SoloConfig,
    _regions: &[usize],
) -> Option<Vec<SoloCage>> {
    let mut cages: Vec<_> = (0..81)
        .map(|i| SoloCage {
            cells: vec![i],
            sum: solution[i] as u16,
        })
        .collect();
    // Independent connected domino/triomino partition. As in source gen_killer_cages,
    // all singleton cages are merged away before logical certification.
    let mut order: Vec<_> = (0..81).collect();
    rng.shuffle(&mut order);
    for i in order {
        let a = cages.iter().position(|k| k.cells.contains(&i)).unwrap();
        if cages[a].cells.len() != 1 {
            continue;
        }
        let mut adjacent = neighbours(i, 9, 9, false);
        rng.shuffle(&mut adjacent);
        // Prefer another singleton, preserving many small sum clues.
        adjacent.sort_by_key(|&j| {
            cages
                .iter()
                .find(|k| k.cells.contains(&j))
                .unwrap()
                .cells
                .len()
        });
        for j in adjacent {
            let b = cages.iter().position(|k| k.cells.contains(&j)).unwrap();
            if a == b
                || cages[b].cells.len() >= 4
                || cages[b].cells.iter().any(|&j| solution[j] == solution[i])
            {
                continue;
            }
            let other = cages.remove(a.max(b));
            let first = &mut cages[a.min(b)];
            first.cells.extend(other.cells);
            first.cells.sort_unstable();
            first.sum += other.sum;
            break;
        }
    }
    cages.iter().all(|k| k.cells.len() > 1).then_some(cages)
}
pub(super) fn generate(rng: &mut Rng, c: SoloVariantConfig) -> Result<Solo, &'static str> {
    c.validate()?;
    let attempts = if c.killer {
        256
    } else if c.base.difficulty >= SoloDifficulty::Extreme {
        96
    } else {
        32
    };
    for _ in 0..attempts {
        let mut regions = box_regions(c.base);
        let mut solution = vec![0; 81];
        let mut r = Rules::new(c.base, regions.clone(), Vec::new());
        if !random_solution(&mut solution, &r, rng, &mut 30_000) {
            continue;
        }
        if c.jigsaw {
            if !jigsaw(&mut regions, &solution, rng) {
                continue;
            }
            r = Rules::new(c.base, regions.clone(), Vec::new());
        }
        let cages = if c.killer {
            let Some(k) = killer_cages(&solution, rng, c.base, &regions) else {
                continue;
            };
            k
        } else {
            Vec::new()
        };
        r.cages = cages.clone();
        r = Rules::new(c.base, regions.clone(), cages.clone());
        let mut digits = if c.killer {
            vec![0; 81]
        } else {
            solution.clone()
        };
        if !c.killer {
            let mut order: Vec<_> = (0..81).collect();
            rng.shuffle(&mut order);
            let mut seen = [false; 81];
            for i in order {
                if seen[i] {
                    continue;
                }
                let cells = orbit(i, 9, c.base.symmetry);
                let saved: Vec<_> = cells.iter().map(|&i| digits[i]).collect();
                for &i in &cells {
                    seen[i] = true;
                    digits[i] = 0;
                }
                if logical(&digits, &r, c.base.difficulty).is_none() {
                    for (&i, &v) in cells.iter().zip(&saved) {
                        digits[i] = v;
                    }
                }
            }
        }
        let Some((answer, mut cert)) = logical(&digits, &r, c.base.difficulty) else {
            continue;
        };
        if answer != solution || cert.achieved != c.base.difficulty {
            continue;
        }
        // Solver labels derive from the easiest permitted replay, not clue count.
        let grades = [
            SoloDifficulty::Trivial,
            SoloDifficulty::Basic,
            SoloDifficulty::Intermediate,
            SoloDifficulty::Advanced,
            SoloDifficulty::Extreme,
        ];
        if grades
            .into_iter()
            .filter(|&g| g < c.base.difficulty)
            .any(|g| logical(&digits, &r, g).is_some())
        {
            continue;
        }
        if c.killer {
            if logical_with_killer(&digits, &r, c.base.difficulty, false).is_some() {
                continue;
            }
            cert.killer_intersection_required = true;
        }
        cert.requested = c.base.difficulty;
        cert.easier_fallback = false;
        return Ok(Solo {
            config: c.base,
            certificate: cert,
            givens: digits.iter().map(|&v| v != 0).collect(),
            digits,
            pencil: vec![0; 81],
            solution,
            regions,
            cages,
        });
    }
    Err("Solo exact logical grade/topology generation budget exhausted; try another seed")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn verify_independently(s: &SoloScene) {
        for region in 0..9 {
            let cells: Vec<_> = (0..81).filter(|&i| s.regions[i] == region).collect();
            assert_eq!(cells.len(), 9);
            let mut reached = vec![cells[0]];
            let mut head = 0;
            while head < reached.len() {
                let i = reached[head];
                head += 1;
                for j in 0..81 {
                    let adjacent = (i / 9 == j / 9 && i.abs_diff(j) == 1) || i.abs_diff(j) == 9;
                    if adjacent && s.regions[j] == region && !reached.contains(&j) {
                        reached.push(j);
                    }
                }
            }
            assert_eq!(reached.len(), 9);
        }
        if !s.cages.is_empty() {
            let mut seen = [false; 81];
            for cage in &s.cages {
                for &i in &cage.cells {
                    assert!(!seen[i]);
                    seen[i] = true;
                }
                let mut reached = vec![cage.cells[0]];
                let mut head = 0;
                while head < reached.len() {
                    let i = reached[head];
                    head += 1;
                    for &j in &cage.cells {
                        if ((i / 9 == j / 9 && i.abs_diff(j) == 1) || i.abs_diff(j) == 9)
                            && !reached.contains(&j)
                        {
                            reached.push(j);
                        }
                    }
                }
                assert_eq!(reached.len(), cage.cells.len());
            }
            assert!(seen.iter().all(|&x| x));
        }
        fn choices(board: &[u8], cell: usize, s: &SoloScene) -> Vec<u8> {
            (1..=9)
                .filter(|&v| {
                    for (j, &digit) in board.iter().enumerate() {
                        if digit != v {
                            continue;
                        }
                        if j / 9 == cell / 9
                            || j % 9 == cell % 9
                            || s.regions[j] == s.regions[cell]
                            || (s.diagonal
                                && ((cell / 9 == cell % 9 && j / 9 == j % 9)
                                    || (cell / 9 + cell % 9 == 8 && j / 9 + j % 9 == 8)))
                        {
                            return false;
                        }
                    }
                    for cage in s.cages.iter().filter(|c| c.cells.contains(&cell)) {
                        let mut sum = v as usize;
                        let mut empty = 0;
                        for &j in &cage.cells {
                            if j == cell {
                                continue;
                            }
                            if board[j] == v {
                                return false;
                            }
                            sum += board[j] as usize;
                            if board[j] == 0 {
                                empty += 1;
                            }
                        }
                        if sum + empty > cage.sum as usize || sum + empty * 9 < (cage.sum as usize)
                        {
                            return false;
                        }
                    }
                    true
                })
                .collect()
        }
        fn count(board: &mut [u8], s: &SoloScene, budget: &mut usize) -> usize {
            assert!(*budget > 0, "independent count exhausted");
            *budget -= 1;
            let best = (0..81)
                .filter(|&i| board[i] == 0)
                .map(|i| (i, choices(board, i, s)))
                .min_by_key(|(_, v)| v.len());
            let Some((i, values)) = best else {
                return 1;
            };
            let mut total = 0;
            for v in values {
                board[i] = v;
                total += count(board, s, budget);
                board[i] = 0;
                if total >= 2 {
                    break;
                }
            }
            total
        }
        assert_eq!(
            count(&mut s.digits.clone(), s, &mut 500_000),
            1,
            "independent row/column/region/diagonal/cage solution count"
        );
    }
    #[test]
    fn remaining_solo_presets_generate_real_certified_boards() {
        for preset in PuzzleKind::Solo.presets().into_iter().filter(|p| {
            p.name.contains("Jigsaw")
                || p.name.contains("Killer")
                || p.name.contains("Intermediate")
                || p.name.contains("Advanced")
                || p.name.contains("Extreme")
                || p.name.contains("Unreasonable")
        }) {
            eprintln!("testing {}", preset.name);
            let cfg = preset.configuration.unwrap();
            let mut successes = 0;
            for seed in 0..4 {
                if let Ok(mut p) = PuzzleSession::new_config(seed, cfg) {
                    eprintln!("success {} seed {}", preset.name, seed);
                    let PuzzleScene::Solo(scene) = p.scene() else {
                        panic!()
                    };
                    assert_eq!(
                        scene.certificate.requested, scene.certificate.achieved,
                        "{}",
                        preset.name
                    );
                    assert!(!scene.certificate.easier_fallback);
                    assert!(scene.digits.contains(&0));
                    assert_eq!(scene.regions.len(), 81);
                    verify_independently(&scene);
                    for i in 0..81 {
                        if scene.symmetry == SoloSymmetry::Rotate2 {
                            assert_eq!(scene.givens[i], scene.givens[80 - i]);
                        }
                    }
                    let initial = p.scene();
                    assert!(!p.assisted());
                    p.solve().unwrap();
                    assert_eq!(p.outcome(), Outcome::Won);
                    assert!(p.assisted());
                    let solved = p.scene();
                    assert!(p.undo());
                    assert_eq!(p.scene(), initial);
                    assert!(!p.assisted());
                    assert!(p.redo());
                    assert_eq!(p.scene(), solved);
                    assert!(p.assisted());
                    p.reset();
                    assert_eq!(p.scene(), initial);
                    assert!(!p.assisted());
                    assert!(!p.can_undo());
                    assert!(!p.can_redo());

                    let rules = Rules::new(
                        SoloConfig {
                            diagonal: scene.diagonal,
                            ..SoloConfig::default()
                        },
                        scene.regions.clone(),
                        scene.cages.clone(),
                    );
                    for lower in [
                        SoloDifficulty::Trivial,
                        SoloDifficulty::Basic,
                        SoloDifficulty::Intermediate,
                        SoloDifficulty::Advanced,
                        SoloDifficulty::Extreme,
                    ]
                    .into_iter()
                    .filter(|&g| g < scene.certificate.requested)
                    {
                        assert!(logical(&scene.digits, &rules, lower).is_none());
                    }

                    if preset.name.contains("Killer") {
                        assert!(scene.cages.iter().all(|c| c.cells.len() > 1));
                        assert!(scene.givens.iter().all(|&g| !g));
                    }
                    successes += 1;
                    break;
                }
            }
            assert!(successes > 0, "{} had no generated boards", preset.name);
        }
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;
    #[test]
    fn intersections_remove_only_candidates_outside_overlap() {
        let r = Rules::new(
            SoloConfig::default(),
            box_regions(SoloConfig::default()),
            Vec::new(),
        );
        let mut p = vec![1022; 81];
        for &i in &[9, 10, 11, 18, 19, 20] {
            p[i] &= !2;
        }
        assert!(intersection(&mut p, &r.units, 9) > 0);
        for &i in &[3, 4, 5, 6, 7, 8] {
            assert_eq!(p[i] & 2, 0);
        }
        assert_eq!(p[0] & 2, 2);
        assert_eq!(p[27] & 2, 2);
    }
    #[test]
    fn full_set_rule_eliminates_naked_triple() {
        let mut p = vec![1022; 9];
        p[0] = 6;
        p[1] = 12;
        p[2] = 10;
        assert!(set_eliminate(&mut p, &(0..9).collect::<Vec<_>>()) > 0);
        assert_eq!(p[3] & 14, 0);
        assert_eq!(p[0], 6);
    }
    #[test]
    fn extreme_fish_eliminates_common_columns() {
        let mut p = vec![1022; 81];
        for y in [0, 1] {
            for x in 0..9 {
                if x != 2 && x != 5 {
                    p[y * 9 + x] &= !2;
                }
            }
        }
        assert!(fish(&mut p, 9) > 0);
        assert_eq!(p[20] & 2, 0);
        assert_eq!(p[23] & 2, 0);
        assert_eq!(p[19] & 2, 2);
    }
    #[test]
    fn extreme_forcing_traverses_bivalue_peer_chain() {
        let r = Rules::new(
            SoloConfig::default(),
            box_regions(SoloConfig::default()),
            Vec::new(),
        );
        let mut p = vec![0; 81];
        p[0] = 6;
        p[3] = 12;
        p[30] = 10;
        p[27] = 30;
        assert_eq!(forcing(&mut p, &r), 1);
        assert_eq!(p[27] & 2, 0);
    }
    #[test]
    fn search_exhaustion_never_certifies_uniqueness() {
        let r = Rules::new(
            SoloConfig::default(),
            box_regions(SoloConfig::default()),
            Vec::new(),
        );
        let mut budget = 1;
        let mut first = None;
        assert_ne!(
            solution_count(&mut [0; 81], &r, 2, &mut budget, &mut first),
            1
        );
        assert_eq!(budget, 0);
    }
    #[test]
    fn killer_distinct_sum_table_matches_independent_enumeration() {
        for size in 2..=4 {
            for sum in 0..=45 {
                for &bits in sum_masks(size, sum) {
                    assert_eq!(bits.count_ones() as usize, size);
                    assert_eq!((1..=9).filter(|v| bits & (1 << v) != 0).sum::<usize>(), sum);
                }
                let expected: Vec<_> = (2u32..1024)
                    .filter(|bits| {
                        bits & 1 == 0
                            && bits.count_ones() as usize == size
                            && (1..=9).filter(|v| bits & (1 << v) != 0).sum::<usize>() == sum
                    })
                    .collect();
                assert_eq!(sum_masks(size, sum), expected);
            }
        }
    }
}
#[cfg(test)]
mod topology_tests {
    use super::*;
    #[test]
    fn jigsaw_peer_conflicts_follow_irregular_regions_instead_of_boxes() {
        let c = SoloVariantConfig {
            base: SoloConfig {
                difficulty: SoloDifficulty::Basic,
                ..SoloConfig::default()
            },
            jigsaw: true,
            killer: false,
        };
        let mut s = generate(&mut Rng(0), c).unwrap();
        let boxes = box_regions(c.base);
        let (a, b) = (0..81)
            .flat_map(|i| (i + 1..81).map(move |j| (i, j)))
            .find(|&(i, j)| {
                i / 9 != j / 9
                    && i % 9 != j % 9
                    && s.regions[i] == s.regions[j]
                    && boxes[i] != boxes[j]
            })
            .unwrap();
        s.digits.fill(0);
        s.digits[a] = 7;
        s.digits[b] = 7;
        assert!(s.conflicts()[a]);
        assert!(s.conflicts()[b]);
        let (a, b) = (0..81)
            .flat_map(|i| (i + 1..81).map(move |j| (i, j)))
            .find(|&(i, j)| {
                i / 9 != j / 9
                    && i % 9 != j % 9
                    && s.regions[i] != s.regions[j]
                    && boxes[i] == boxes[j]
            })
            .unwrap();
        s.digits.fill(0);
        s.digits[a] = 7;
        s.digits[b] = 7;
        assert!(!s.conflicts()[a]);
        assert!(!s.conflicts()[b]);
    }
    #[test]
    fn killer_wrong_sum_blocks_win_even_when_all_sudoku_units_are_valid() {
        let c = SoloVariantConfig {
            base: SoloConfig {
                symmetry: SoloSymmetry::None,
                ..SoloConfig::default()
            },
            jigsaw: false,
            killer: true,
        };
        let mut s = generate(&mut Rng(0), c).unwrap();
        s.digits = s.solution.clone();
        assert_eq!(s.outcome(), Outcome::Won);
        s.cages[0].sum += 1;
        assert_eq!(s.outcome(), Outcome::Playing);
        assert!(s.cages[0].cells.iter().all(|&i| s.conflicts()[i]));
    }
    #[test]
    fn cage_minmax_and_sum_sets_really_prune_candidates() {
        let c = SoloConfig::default();
        let mut cages: Vec<_> = (1..9)
            .map(|y| SoloCage {
                cells: (0..9).map(|x| y * 9 + x).collect(),
                sum: 45,
            })
            .collect();
        cages.push(SoloCage {
            cells: (2..9).collect(),
            sum: 42,
        });
        cages.push(SoloCage {
            cells: vec![0, 1],
            sum: 3,
        });
        let r = Rules::new(c, box_regions(c), cages);
        let mut p = vec![1022; 81];
        let (_, changes) = killer_constraints(&[0; 81], &mut p, &r, true).unwrap();
        assert!(changes > 0);
        assert_eq!(p[0], 6);
        assert_eq!(p[1], 6);
    }
    #[test]
    fn killer_stock_requires_source_intersection_grade() {
        let c = SoloVariantConfig {
            base: SoloConfig {
                symmetry: SoloSymmetry::None,
                ..SoloConfig::default()
            },
            jigsaw: false,
            killer: true,
        };
        let s = generate(&mut Rng(0), c).unwrap();
        let r = Rules::new(c.base, s.regions.clone(), s.cages.clone());
        assert!(s.certificate.killer_intersection_required);
        assert!(logical_with_killer(&s.digits, &r, SoloDifficulty::Trivial, false).is_none());
        assert!(logical(&s.digits, &r, SoloDifficulty::Trivial).is_some());
    }
    #[test]
    fn unsupported_advanced_size_is_rejected_before_allocation() {
        let c = SoloConfig {
            block_width: 4,
            block_height: 4,
            difficulty: SoloDifficulty::Extreme,
            ..SoloConfig::default()
        };
        assert!(c.validate().is_err());
        assert!(Solo::new_config(&mut Rng(0), c).is_err());
    }
}
