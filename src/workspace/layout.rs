//! Layout geometry contains stable terminal IDs, never connections or terminal owners.
use eframe::egui::{Rect, Vec2, vec2};
use serde::{Deserialize, Serialize};
pub const MIN_WIDTH: f32 = 240.;
pub const MIN_HEIGHT: f32 = 220.;
pub const DIVIDER: f32 = 6.;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutPreset {
    #[default]
    Single,
    Columns,
    Rows,
    Four,
}
impl LayoutPreset {
    pub fn capacity(self) -> usize {
        match self {
            Self::Single => 1,
            Self::Columns | Self::Rows => 2,
            Self::Four => 4,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPreferences {
    pub preset: LayoutPreset,
    pub ratios: [f32; 3],
}
impl Default for LayoutPreferences {
    fn default() -> Self {
        Self {
            preset: LayoutPreset::Single,
            ratios: [0.5; 3],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayoutTree {
    Leaf {
        id: u64,
    },
    Split {
        axis: Orientation,
        ratio: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}
pub struct PaneRect {
    pub id: u64,
    pub rect: Rect,
}
pub struct Divider {
    pub path: Vec<bool>,
    pub axis: Orientation,
    pub rect: Rect,
    pub parent: Rect,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutState {
    pub root: Option<LayoutTree>,
    pub active: Option<u64>,
}
impl LayoutTree {
    fn split(axis: Orientation, ratio: f32, first: Self, second: Self) -> Self {
        Self::Split {
            axis,
            ratio: if ratio.is_finite() {
                ratio.clamp(0.05, 0.95)
            } else {
                0.5
            },
            first: Box::new(first),
            second: Box::new(second),
        }
    }
    pub fn preset(ids: &[u64], prefs: &LayoutPreferences) -> Option<Self> {
        let mut unique = Vec::new();
        for id in ids {
            if !unique.contains(id) {
                unique.push(*id);
            }
            if unique.len() == prefs.preset.capacity() {
                break;
            }
        }
        let leaf = |index| Self::Leaf { id: unique[index] };
        Some(match unique.len() {
            0 => return None,
            1 => leaf(0),
            2 => Self::split(
                if prefs.preset == LayoutPreset::Rows {
                    Orientation::Vertical
                } else {
                    Orientation::Horizontal
                },
                if prefs.preset == LayoutPreset::Four {
                    prefs.ratios[1]
                } else {
                    prefs.ratios[0]
                },
                leaf(0),
                leaf(1),
            ),
            _ => Self::split(
                Orientation::Vertical,
                prefs.ratios[0],
                Self::split(Orientation::Horizontal, prefs.ratios[1], leaf(0), leaf(1)),
                if unique.len() == 3 {
                    leaf(2)
                } else {
                    Self::split(Orientation::Horizontal, prefs.ratios[2], leaf(2), leaf(3))
                },
            ),
        })
    }
    pub fn ids(&self) -> Vec<u64> {
        match self {
            Self::Leaf { id } => vec![*id],
            Self::Split { first, second, .. } => {
                let mut ids = first.ids();
                ids.extend(second.ids());
                ids
            }
        }
    }
    pub fn minimum_size(&self) -> Vec2 {
        match self {
            Self::Leaf { .. } => vec2(MIN_WIDTH, MIN_HEIGHT),
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                let a = first.minimum_size();
                let b = second.minimum_size();
                match axis {
                    Orientation::Horizontal => vec2(a.x + b.x + DIVIDER, a.y.max(b.y)),
                    Orientation::Vertical => vec2(a.x.max(b.x), a.y + b.y + DIVIDER),
                }
            }
        }
    }
    pub fn fits(&self, bounds: Rect) -> bool {
        let min = self.minimum_size();
        bounds.is_finite() && bounds.width() >= min.x && bounds.height() >= min.y
    }
    fn parts(&self, bounds: Rect) -> Option<(Rect, Rect, Rect)> {
        let Self::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        else {
            return None;
        };
        let horizontal = *axis == Orientation::Horizontal;
        let extent = if horizontal {
            bounds.width()
        } else {
            bounds.height()
        }
        .max(0.);
        let gap = DIVIDER.min(extent);
        let available = extent - gap;
        let min = first.minimum_size();
        let other = second.minimum_size();
        let (a, b) = if horizontal {
            (min.x, other.x)
        } else {
            (min.y, other.y)
        };
        let ratio = if ratio.is_finite() {
            ratio.clamp(0., 1.)
        } else {
            0.5
        };
        let size = if available >= a + b {
            (available * ratio).clamp(a, available - b)
        } else {
            available * ratio
        };
        let mut first = bounds;
        let mut second = bounds;
        let mut divider = bounds;
        if horizontal {
            first.max.x = bounds.min.x + size;
            divider.min.x = first.max.x;
            divider.max.x = divider.min.x + gap;
            second.min.x = divider.max.x;
        } else {
            first.max.y = bounds.min.y + size;
            divider.min.y = first.max.y;
            divider.max.y = divider.min.y + gap;
            second.min.y = divider.max.y;
        }
        Some((first, divider, second))
    }
    pub fn layout(&self, bounds: Rect) -> Vec<PaneRect> {
        match self {
            Self::Leaf { id } => vec![PaneRect {
                id: *id,
                rect: bounds,
            }],
            Self::Split { first, second, .. } => {
                let (a, _, b) = self.parts(bounds).unwrap();
                let mut panes = first.layout(a);
                panes.extend(second.layout(b));
                panes
            }
        }
    }
    pub fn dividers(&self, bounds: Rect) -> Vec<Divider> {
        let mut output = Vec::new();
        self.collect_dividers(bounds, &mut Vec::new(), &mut output);
        output
    }
    fn collect_dividers(&self, bounds: Rect, path: &mut Vec<bool>, output: &mut Vec<Divider>) {
        if let Self::Split {
            axis,
            first,
            second,
            ..
        } = self
        {
            let (a, rect, b) = self.parts(bounds).unwrap();
            output.push(Divider {
                path: path.clone(),
                axis: *axis,
                rect,
                parent: bounds,
            });
            path.push(false);
            first.collect_dividers(a, path, output);
            path.pop();
            path.push(true);
            second.collect_dividers(b, path, output);
            path.pop();
        }
    }
    pub fn set_ratio(&mut self, path: &[bool], ratio: f32) -> Result<(), String> {
        if !ratio.is_finite() {
            return Err("Divider ratio must be finite.".into());
        }
        match self {
            Self::Split {
                ratio: current,
                first,
                second,
                ..
            } => {
                if let Some((right, rest)) = path.split_first() {
                    if *right {
                        second.set_ratio(rest, ratio)
                    } else {
                        first.set_ratio(rest, ratio)
                    }
                } else {
                    *current = ratio.clamp(0.05, 0.95);
                    Ok(())
                }
            }
            Self::Leaf { .. } => Err("No divider at this layout path.".into()),
        }
    }
    pub fn remove(self, id: u64) -> Option<Self> {
        match self {
            Self::Leaf { id: current } => (current != id).then_some(Self::Leaf { id: current }),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(id), second.remove(id)) {
                (Some(first), Some(second)) => Some(Self::split(axis, ratio, first, second)),
                (Some(tree), None) | (None, Some(tree)) => Some(tree),
                (None, None) => None,
            },
        }
    }
    pub fn replace(&mut self, old: u64, new: u64) -> bool {
        if self.ids().contains(&new) {
            return false;
        }
        match self {
            Self::Leaf { id } => {
                if *id == old {
                    *id = new;
                    true
                } else {
                    false
                }
            }
            Self::Split { first, second, .. } => {
                first.replace(old, new) || second.replace(old, new)
            }
        }
    }
}
impl LayoutState {
    pub fn remove(&mut self, id: u64) {
        self.root = self.root.take().and_then(|tree| tree.remove(id));
        if self.active == Some(id) {
            self.active = self
                .root
                .as_ref()
                .and_then(|tree| tree.ids().first().copied());
        }
    }
    pub fn focus(&mut self, id: u64) {
        if let Some(tree) = &mut self.root {
            if !tree.ids().contains(&id) {
                let old = self.active.or_else(|| tree.ids().first().copied());
                if let Some(old) = old {
                    tree.replace(old, id);
                }
            }
        } else {
            self.root = Some(LayoutTree::Leaf { id });
        }
        self.active = Some(id);
    }
}
impl LayoutPreferences {
    pub fn capture(&mut self, tree: &LayoutTree) {
        if let LayoutTree::Split {
            ratio,
            first,
            second,
            ..
        } = tree
        {
            if self.preset == LayoutPreset::Four && tree.ids().len() == 2 {
                self.ratios[1] = *ratio;
            } else {
                self.ratios[0] = *ratio;
                if let LayoutTree::Split { ratio, .. } = first.as_ref() {
                    self.ratios[1] = *ratio;
                }
                if let LayoutTree::Split { ratio, .. } = second.as_ref() {
                    self.ratios[2] = *ratio;
                }
            }
        }
    }
}
