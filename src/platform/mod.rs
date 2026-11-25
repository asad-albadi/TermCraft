pub mod shortcuts;

pub(crate) mod private_file;
#[cfg(windows)]
pub(crate) mod windows_rename;

#[allow(dead_code)]
#[path = "../remote_files/transfer/operations/commit.rs"]
mod atomic_rename;
pub(crate) fn publish_no_replace(
    source: &std::path::Path,
    target: &std::path::Path,
) -> std::io::Result<()> {
    atomic_rename::rename_no_replace(source, target)
}

pub mod launch;

pub mod linux_integration;
pub mod macos_integration;

pub mod path_map;
pub mod registry_fs;
pub mod windows_integration;
