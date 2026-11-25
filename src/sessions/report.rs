use crate::model::{Session, Store};
#[derive(Clone, Debug)]
pub struct SourceLocation {
    pub line: usize,
    pub section: String,
    pub entry: String,
}
impl SourceLocation {
    pub fn new(line: usize, section: &str, entry: &str) -> Self {
        Self {
            line,
            section: section.chars().take(256).collect(),
            entry: entry.chars().take(256).collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct UnsupportedField {
    pub field: String,
    pub message: String,
    pub location: SourceLocation,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub session: Session,
    pub location: SourceLocation,
}
#[derive(Clone, Debug)]
pub struct ImportReport {
    pub format: String,
    pub encoding: String,
    pub candidates: Vec<Candidate>,
    pub unsupported: Vec<UnsupportedField>,
    pub omitted_warnings: usize,
}
impl ImportReport {
    pub fn new(format: &str, encoding: String) -> Self {
        Self {
            format: format.into(),
            encoding,
            candidates: Vec::new(),
            unsupported: Vec::new(),
            omitted_warnings: 0,
        }
    }
    pub fn warn(&mut self, field: &str, message: &str, location: &SourceLocation) {
        if self.unsupported.len() < 1000 {
            self.unsupported.push(UnsupportedField {
                field: field.chars().take(256).collect(),
                message: message.into(),
                location: location.clone(),
            });
        } else {
            self.omitted_warnings += 1;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DuplicatePolicy {
    #[default]
    Skip,
    Replace,
    KeepBoth,
}
#[derive(Debug, Default)]
pub struct ApplySummary {
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
}
pub fn validate(session: &Session) -> Result<(), String> {
    crate::model::validate_host(&session.host)?;
    crate::model::validate_user(&session.user)?;
    if session.port == 0 {
        return Err("Port must be between 1 and 65535".into());
    }
    for value in [&session.name, &session.folder, &session.key] {
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(
                "Session text contains unsupported control characters or exceeds 4096 bytes".into(),
            );
        }
    }
    if session.name.trim().is_empty() {
        return Err("Session name is empty".into());
    }
    Ok(())
}
fn same(a: &Session, b: &Session) -> bool {
    a.folder == b.folder && a.name == b.name
}
pub fn apply(
    store: &mut Store,
    report: ImportReport,
    policy: DuplicatePolicy,
) -> Result<ApplySummary, String> {
    for entry in &report.candidates {
        validate(&entry.session)?;
    }
    let mut sessions = store.sessions.clone();
    let mut summary = ApplySummary::default();
    for entry in report.candidates {
        let mut session = entry.session;
        let existing: Vec<_> = sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| same(s, &session))
            .map(|(i, _)| i)
            .collect();
        if !existing.is_empty() {
            match policy {
                DuplicatePolicy::Skip => {
                    summary.skipped += 1;
                    continue;
                }
                DuplicatePolicy::Replace => {
                    if existing.len() != 1 {
                        return Err(
                            "More than one local session matches; choose Skip or Keep both".into(),
                        );
                    }
                    let index = existing[0];
                    session.id = sessions[index].id.clone();
                    sessions[index] = session;
                    summary.replaced += 1;
                    continue;
                }
                DuplicatePolicy::KeepBoth => {
                    let original = session.name.clone();
                    let mut n = 2;
                    while sessions.iter().any(|s| same(s, &session)) {
                        session.name = format!("{original} (imported {n})");
                        n += 1;
                    }
                }
            }
        }
        session.id = crate::model::new_session_id();
        sessions.push(session);
        summary.added += 1;
    }
    store.sessions = sessions;
    Ok(summary)
}
