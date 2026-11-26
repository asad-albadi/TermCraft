//! Approval is a compiled trust decision. A folder-local JSON file cannot opt in
//! an unreviewed executable or replace the checksum of a private build.
pub const WINDOWS_ARTIFACT: &str = "vcxsrv-21.1.16.1-termcraft-private1-windows-x86_64";
pub const MACOS_ARTIFACT: &str = "xquartz-2.8.6-termcraft-private1-macos-arm64";
#[cfg(target_os = "macos")]
pub const MACOS_RUNTIME: &str = "x11/macos-arm64-private1";
pub struct NativePin {
    pub files: &'static [(&'static str, &'static str)],
    pub executable: &'static str,
    pub version: &'static str,
}
pub fn windows_pin() -> Result<&'static NativePin, String> {
    Err("The private Windows X server has no approved native build yet. Stock VcXsrv binds all network interfaces and cannot satisfy owned exclusive loopback transport. Build the pinned private transport source and qualify its executable/dependencies on Windows before packaging; System compatibility remains an explicit alternative.".into())
}
pub fn macos_pin() -> Result<&'static NativePin, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        static PIN: std::sync::LazyLock<NativePin> = std::sync::LazyLock::new(|| NativePin {
            executable: "XQuartz.app/Contents/MacOS/X11.bin",
            version: MACOS_ARTIFACT,
            files: include_str!("../../packaging/x11/native/macos-arm64.sha256")
                .lines()
                .map(|line| {
                    let (hash, path) = line
                        .split_once("  ")
                        .expect("valid compiled XQuartz inventory");
                    (path, hash)
                })
                .collect::<Vec<_>>()
                .leak(),
        });
        Ok(&PIN)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    Err("The approved private XQuartz engine requires Apple Silicon macOS.".into())
}
