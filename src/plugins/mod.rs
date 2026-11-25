//! TermCraft plugins installed from inspected, content-pinned local bundles.
mod host;
mod install;
mod manifest;
mod registry;

pub use install::ReviewedPlugin;
pub use manifest::{HOST_PROTOCOL, Permission, PluginManifest};
pub use registry::{InstalledPlugin, PluginRegistry, Removal};

use crate::packages::PackageError;

pub type Result<T> = std::result::Result<T, PackageError>;

fn invalid(message: impl Into<String>) -> PackageError {
    PackageError::Invalid(message.into())
}

#[cfg(windows)]
pub fn windows_bootstrap() -> Option<i32> {
    host::run_if_requested()
}
