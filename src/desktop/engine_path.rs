//! RDP helper location policy and actionable process-launch errors.
use std::{
    io,
    path::{Path, PathBuf},
};
fn name() -> &'static str {
    if cfg!(windows) {
        "termcraft-rdp-engine.exe"
    } else {
        "termcraft-rdp-engine"
    }
}
pub(crate) fn resolve(executable: Option<&Path>, root: &Path, development: bool) -> PathBuf {
    let adjacent = executable
        .map(|exe| exe.with_file_name(name()))
        .unwrap_or_default();
    if development && !adjacent.try_exists().unwrap_or(true) {
        return root.join("engines/rdp/target/debug").join(name());
    }
    adjacent
}
pub(crate) fn launch_error(path: &Path, error: &io::Error) -> String {
    if error.kind() == io::ErrorKind::NotFound
        && matches!(std::fs::metadata(path),Err(ref e) if e.kind()==io::ErrorKind::NotFound)
    {
        format!(
            "RDP helper is missing from this TermCraft build. Expected helper: {}. {error}",
            path.display()
        )
    } else {
        format!(
            "RDP helper could not start. Helper: {}. {error}",
            path.display()
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_development_review_uses_its_adjacent_helper() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("TermCraft");
        let helper = exe.with_file_name(name());
        std::fs::write(&helper, b"fixture").unwrap();
        assert_eq!(
            resolve(Some(&exe), Path::new("unrelated-source-tree"), true),
            helper
        );
    }
    #[test]
    fn ordinary_development_keeps_source_tree_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("TermCraft");
        let root = dir.path().join("workspace");
        assert_eq!(
            resolve(Some(&exe), &root, true),
            root.join("engines/rdp/target/debug").join(name())
        );
    }
    #[test]
    fn release_never_uses_the_development_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("TermCraft");
        assert_eq!(
            resolve(Some(&exe), dir.path(), false),
            exe.with_file_name(name())
        );
    }
    #[test]
    fn missing_helper_and_missing_loader_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name());
        let error = io::Error::from(io::ErrorKind::NotFound);
        assert!(
            launch_error(&path, &error)
                .starts_with("RDP helper is missing from this TermCraft build.")
        );
        std::fs::write(&path, b"fixture").unwrap();
        assert!(launch_error(&path, &error).starts_with("RDP helper could not start."));
    }
}
