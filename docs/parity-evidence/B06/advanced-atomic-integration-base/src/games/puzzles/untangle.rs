use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UntangleScene {
    pub extent: i32,
    pub vertices: Vec<(i32, i32)>,
    pub edges: Vec<(usize, usize)>,
    pub crossing_edges: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Untangle {
    vertices: Vec<(i32, i32)>,
    edges: Vec<(usize, usize)>,
    solution: Vec<(i32, i32)>,
}
fn orient(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i64 {
    (b.0 - a.0) as i64 * (c.1 - a.1) as i64 - (b.1 - a.1) as i64 * (c.0 - a.0) as i64
}
fn on(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> bool {
    orient(a, b, p) == 0
        && (a.0.min(b.0)..=a.0.max(b.0)).contains(&p.0)
        && (a.1.min(b.1)..=a.1.max(b.1)).contains(&p.1)
}
fn intersect(a: (i32, i32), b: (i32, i32), c: (i32, i32), d: (i32, i32)) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    (o1.signum() != o2.signum()
        && o3.signum() != o4.signum()
        && o1 != 0
        && o2 != 0
        && o3 != 0
        && o4 != 0)
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}
impl Untangle {
    pub(super) fn solve(&mut self) {
        self.vertices = self.solution.clone();
    }
    pub fn new(rng: &mut Rng) -> Self {
        Self::new_config(rng, 10).expect("valid default")
    }
    pub fn new_config(rng: &mut Rng, points: usize) -> Result<Self, &'static str> {
        if !(4..=25).contains(&points) {
            return Err("Untangle supports 4 through 25 points");
        }
        let solution = if points == 10 {
            vec![
                (500, 80),
                (747, 160),
                (900, 370),
                (900, 630),
                (747, 840),
                (500, 920),
                (253, 840),
                (100, 630),
                (100, 370),
                (253, 160),
            ]
        } else {
            (0..points)
                .map(|i| {
                    let angle = std::f64::consts::TAU * (i as f64) / (points as f64);
                    (
                        500 + (420.0 * angle.sin()).round() as i32,
                        500 - (420.0 * angle.cos()).round() as i32,
                    )
                })
                .collect()
        };
        let mut edges: Vec<_> = (0..points).map(|i| (i, (i + 1) % points)).collect();
        let mut polygon: Vec<_> = (0..points).collect();
        while polygon.len() > 3 {
            let i = rng.below(polygon.len());
            let a = polygon[(i + polygon.len() - 1) % polygon.len()];
            let b = polygon[(i + 1) % polygon.len()];
            edges.push((a, b));
            polygon.remove(i);
        }
        let mut vertices = solution.clone();
        rng.shuffle(&mut vertices);
        let mut s = Self {
            vertices,
            edges,
            solution,
        };
        if s.outcome() == Outcome::Won {
            s.vertices = s.solution.clone();
            s.vertices.swap(1, points / 2);
        }
        Ok(s)
    }
    fn crossings(&self) -> Vec<bool> {
        let mut marked = vec![false; self.edges.len()];
        for (i, &(a, b)) in self.edges.iter().enumerate() {
            for (j, &(c, d)) in self.edges.iter().enumerate().skip(i + 1) {
                // Historical mark_crossings ignores edge pairs sharing a graph vertex.
                if a == c || a == d || b == c || b == d {
                    continue;
                }
                let crosses = intersect(
                    self.vertices[a],
                    self.vertices[b],
                    self.vertices[c],
                    self.vertices[d],
                );
                if crosses {
                    marked[i] = true;
                    marked[j] = true;
                }
            }
        }
        marked
    }
    pub fn outcome(&self) -> Outcome {
        if self.crossings().contains(&true) {
            Outcome::Playing
        } else {
            Outcome::Won
        }
    }
    pub fn scene(&self) -> UntangleScene {
        UntangleScene {
            extent: 1000,
            vertices: self.vertices.clone(),
            edges: self.edges.clone(),
            crossing_edges: self.crossings(),
        }
    }
    pub fn apply(&mut self, input: PuzzleInput) -> Result<(), &'static str> {
        let PuzzleInput::Move { vertex, x, y } = input else {
            return Err("Unsupported Untangle input");
        };
        if vertex >= self.vertices.len() || !(0..=1000).contains(&x) || !(0..=1000).contains(&y) {
            return Err("Point outside board");
        }
        self.vertices[vertex] = (x, y);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_playthrough_and_degeneracy() {
        for seed in 0..100 {
            let mut s = Untangle::new(&mut Rng(seed));
            assert_eq!(s.outcome(), Outcome::Playing);
            assert_eq!(s.edges.len(), 17);
            for i in 0..10 {
                let (x, y) = s.solution[i];
                s.apply(PuzzleInput::Move { vertex: i, x, y }).unwrap();
            }
            assert_eq!(s.outcome(), Outcome::Won);
            s.vertices[0] = s.vertices[1];
            assert_eq!(s.outcome(), Outcome::Playing);
            assert!(
                s.apply(PuzzleInput::Move {
                    vertex: usize::MAX,
                    x: 0,
                    y: 0
                })
                .is_err()
            );
            assert!(
                s.apply(PuzzleInput::Move {
                    vertex: 0,
                    x: i32::MIN,
                    y: 0
                })
                .is_err()
            );
        }
    }
    #[test]
    fn edge_touch_and_overlap() {
        assert!(intersect((0, 0), (10, 0), (5, 0), (5, 5)));
        assert!(intersect((0, 0), (10, 0), (5, 0), (12, 0)));
        assert!(!intersect((0, 0), (3, 0), (5, 0), (12, 0)));
    }
}
#[cfg(test)]
mod recovered_fixtures {
    use super::*;
    // f306b9d untangle.c cross (endpoint-on-line counts) and mark_crossings (shared graph endpoint skipped).
    #[test]
    fn recovered_crossing_and_shared_endpoint_rules() {
        let mut u = Untangle {
            vertices: vec![(0, 0), (1000, 1000), (0, 1000), (1000, 0)],
            edges: vec![(0, 1), (2, 3)],
            solution: Vec::new(),
        };
        assert_eq!(u.outcome(), Outcome::Playing);
        u.apply(PuzzleInput::Move {
            vertex: 1,
            x: 1000,
            y: 0,
        })
        .unwrap();
        assert_eq!(u.outcome(), Outcome::Playing); // nonincident endpoint touch
        u.vertices = vec![(0, 0), (1000, 0), (0, 1000), (1000, 1000)];
        assert_eq!(u.outcome(), Outcome::Won);
        u.vertices = vec![(0, 0), (1000, 0), (500, 0)];
        u.edges = vec![(0, 1), (0, 2)];
        assert_eq!(u.outcome(), Outcome::Won); // exact historical shared-vertex exemption
        assert_eq!(Untangle::new(&mut Rng(0)).scene().vertices.len(), 10);
    }
}
#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn stock_point_counts_have_valid_solved_embeddings() {
        for points in [6, 10, 15, 20, 25] {
            let mut u = Untangle::new_config(&mut Rng(4), points).unwrap();
            assert_eq!(u.vertices.len(), points);
            assert_eq!(u.edges.len(), 2 * points - 3);
            assert_eq!(u.outcome(), Outcome::Playing);
            u.solve();
            assert_eq!(u.outcome(), Outcome::Won);
        }
    }
}
