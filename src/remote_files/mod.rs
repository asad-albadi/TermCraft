//! Remote filesystem identity is independent of the local operating system.
mod path;
mod state;
pub use path::RemotePath;
pub use state::{BrowserFollowMode, BrowserState, DirectorySnapshot, EntryKind, RemoteEntry};

pub mod capabilities;
pub mod editor;
pub mod scp;
pub mod transfer;
