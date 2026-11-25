//! Explicit target membership; no authentication or transport ownership.
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOrigin {
    TerminalKeyboard,
    Authentication,
    FileOperation,
    ProtocolReply,
}
#[derive(Default, Debug)]
pub struct BroadcastReport {
    pub sent: Vec<u64>,
    pub failed: Vec<(u64, String)>,
}
#[derive(Default)]
pub struct BroadcastGroup {
    selected: BTreeSet<u64>,
}
impl BroadcastGroup {
    pub fn select(&mut self, id: u64, selected: bool) {
        if selected {
            self.selected.insert(id);
        } else {
            self.selected.remove(&id);
        }
    }
    pub fn selected(&self) -> Vec<u64> {
        self.selected.iter().copied().collect()
    }
    pub fn targets(&self, source: u64) -> Vec<u64> {
        if self.selected.contains(&source) {
            self.selected()
        } else {
            vec![source]
        }
    }
    pub fn retain_connected(&mut self, connected: &[u64]) -> Vec<u64> {
        let removed = self
            .selected
            .iter()
            .copied()
            .filter(|id| !connected.contains(id))
            .collect();
        self.selected.retain(|id| connected.contains(id));
        removed
    }
    pub fn dispatch(
        &self,
        input: &[u8],
        send: impl FnMut(u64, &[u8]) -> Result<(), String>,
    ) -> BroadcastReport {
        self.dispatch_input(InputOrigin::TerminalKeyboard, input, send)
    }
    pub fn dispatch_input(
        &self,
        origin: InputOrigin,
        input: &[u8],
        mut send: impl FnMut(u64, &[u8]) -> Result<(), String>,
    ) -> BroadcastReport {
        let mut report = BroadcastReport::default();
        if origin != InputOrigin::TerminalKeyboard || input.is_empty() {
            return report;
        }
        for id in &self.selected {
            match send(*id, input) {
                Ok(()) => report.sent.push(*id),
                Err(error) => report.failed.push((*id, error)),
            }
        }
        report
    }
}
