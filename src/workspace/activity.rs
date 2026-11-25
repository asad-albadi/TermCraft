use std::collections::BTreeMap;
#[derive(Default)]
struct Status {
    unread: bool,
    error: bool,
}
#[derive(Default)]
pub struct ActivityState {
    states: BTreeMap<u64, Status>,
}
impl ActivityState {
    pub fn on_output(&mut self, id: u64, visible: bool) {
        self.states.entry(id).or_default().unread = !visible;
    }
    pub fn mark_seen(&mut self, id: u64) {
        if let Some(s) = self.states.get_mut(&id) {
            s.unread = false;
        }
    }
    pub fn unread(&self, id: u64) -> bool {
        self.states.get(&id).is_some_and(|s| s.unread)
    }
    pub fn set_error(&mut self, id: u64, error: bool) {
        self.states.entry(id).or_default().error = error;
    }
    pub fn error(&self, id: u64) -> bool {
        self.states.get(&id).is_some_and(|s| s.error)
    }
    pub fn remove(&mut self, id: u64) {
        self.states.remove(&id);
    }
}
