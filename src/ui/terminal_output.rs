use super::*;
use std::sync::mpsc::{Receiver, TryRecvError};
pub(super) const TICK_BYTES: usize = 8192;
pub(super) fn drain(
    output: &mut Vec<u8>,
    budget: usize,
    pending: &mut Option<(Vec<u8>, usize)>,
    rx: &Receiver<Vec<u8>>,
) -> bool {
    for _ in 0..256 {
        if output.len() >= budget {
            break;
        }
        if pending.is_none() {
            match rx.try_recv() {
                Ok(bytes) => *pending = Some((bytes, 0)),
                Err(TryRecvError::Disconnected) => return true,
                Err(_) => break,
            }
        }
        let (bytes, offset) = pending.as_mut().unwrap();
        let take = (budget - output.len()).min(bytes.len() - *offset);
        output.extend_from_slice(&bytes[*offset..*offset + take]);
        *offset += take;
        if *offset == bytes.len() {
            *pending = None;
        } else {
            break;
        }
    }
    false
}
impl Workspace {
    pub(super) fn pump_terminals(&mut self) {
        let count = self.tabs.len();
        if count == 0 {
            return;
        }
        let mut budget = TICK_BYTES;
        for step in 0..count {
            let index = (self.poll_cursor + step) % count;
            let tab = &mut self.tabs[index];
            tab.apply_terminal_preferences(&self.store.preferences);
            let used = tab.poll_with_budget(budget);
            budget = budget.saturating_sub(used);
            tab.retry_mouse();
            tab.tick_paste(Instant::now());
            self.activity.set_error(tab.id, !tab.error.is_empty());
            if std::mem::take(&mut tab.output_changed) {
                self.activity.on_output(tab.id, false);
            }
        }
        self.poll_cursor = (self.poll_cursor + 1) % count;
        self.reconcile_broadcast();
    }
}
