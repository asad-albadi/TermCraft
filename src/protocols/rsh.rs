// Rsh's initial NUL selects a single connection. The server multiplexes
// stderr with stdout and does not open a reverse connection for signals.
pub(super) fn greeting(local: &str, remote: &str, command: &str) -> Vec<u8> {
    format!("\0{local}\0{remote}\0{command}\0").into_bytes()
}
