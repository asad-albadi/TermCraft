//! Group launches report each attempted session independently.
use crate::model::{Session, SessionId, Store};
#[derive(Clone, Debug)]
pub struct GroupFailure {
    pub id: SessionId,
    pub name: String,
    pub error: String,
}
#[derive(Clone, Debug, Default)]
pub struct GroupReport {
    pub accepted: Vec<SessionId>,
    pub failed: Vec<GroupFailure>,
}
pub struct SessionGroup;
impl SessionGroup {
    /// Folder names use `/` hierarchy. `Team` includes `Team/Sub`, never `Teamwork`.
    pub fn resolve(folder: &str, store: &Store) -> Vec<Session> {
        let folder = folder.trim_end_matches('/');
        store
            .sessions
            .iter()
            .filter(|s| {
                s.folder == folder
                    || s.folder
                        .strip_prefix(folder)
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .cloned()
            .collect()
    }
    /// Accepted means a launch was submitted, not that remote authentication succeeded.
    pub fn launch(
        folder: &str,
        store: &Store,
        mut launch: impl FnMut(Session) -> Result<(), String>,
    ) -> GroupReport {
        let mut report = GroupReport::default();
        for session in Self::resolve(folder, store) {
            let id = session.id.clone();
            let name = session.title();
            match launch(session) {
                Ok(()) => report.accepted.push(id),
                Err(error) => report.failed.push(GroupFailure { id, name, error }),
            }
        }
        report
    }
}
