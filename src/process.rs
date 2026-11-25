#[path = "process_input.rs"]
mod input;
pub use input::InputQueue;
#[cfg(unix)]
#[path = "process_unix.rs"]
mod backend;
#[cfg(windows)]
#[path = "process_windows.rs"]
mod backend;
pub use backend::Process;
