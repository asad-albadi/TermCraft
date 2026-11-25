use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct SearchIndex {
    pub revision: Option<u64>,
    pub builds: u64,
    pub query: String,
    pub count: usize,
    pub history_start: u64,
    pub history_counts: VecDeque<usize>,
    pub history_total: usize,
    pub indexed_history_lines: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct ScreenStats {
    pub history_bytes: usize,
    pub history_budget: usize,
    pub discarded_lines: u64,
    pub index_builds: u64,
    pub indexed_history_lines: u64,
}
pub(super) const DEFAULT_HISTORY_BUDGET: usize = 128 * 1024 * 1024;
