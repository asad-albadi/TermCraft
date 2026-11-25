//! Approval is a compiled trust decision. A folder-local JSON file cannot opt in
//! an unreviewed executable or replace the checksum of a private build.
pub const WINDOWS_ARTIFACT: &str = "vcxsrv-21.1.16.1-termcraft-private1-windows-x86_64";
pub const MACOS_ARTIFACT: &str = "xquartz-2.8.6-termcraft-private1-macos-universal";
pub struct NativePin<'a> {
    pub files: &'a [(&'a str, &'a str)],
    pub executable: &'static str,
    pub version: &'static str,
}
pub fn windows_pin() -> Result<&'static NativePin<'static>, String> {
    Err("The private Windows X server has no approved native build yet. Stock VcXsrv binds all network interfaces and cannot satisfy owned exclusive loopback transport. Build the pinned private transport source and qualify its executable/dependencies on Windows before packaging; System compatibility remains an explicit alternative.".into())
}
pub fn macos_pin() -> Result<&'static NativePin<'static>, String> {
    Err("The private macOS X server has no approved relocated and signed native build yet. Stock XQuartz depends on /opt/X11 and system Mach/startx services; its inherited-listener path bypasses Xauthority. Build the pinned private direct-entry/authenticated-listener source and qualify it on macOS before packaging; System compatibility remains an explicit alternative.".into())
}
