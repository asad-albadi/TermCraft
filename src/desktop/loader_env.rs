//! Keep native loader overrides out of desktop helper processes.
pub(crate) fn sanitize(command: &mut std::process::Command) {
    let overridden: Vec<_> = std::env::vars_os()
        .map(|(name, _)| name)
        .chain(command.get_envs().map(|(name, _)| name.to_owned()))
        .filter(|name| {
            let name = name.to_string_lossy();
            [
                "LD_",
                "DYLD_",
                "PCSCLITE_",
                "LIBPCSCLITE_",
                "TERMCRAFT_TEST_PCSC_",
                "TERMCRAFT_PCSC_FIXTURE_",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
                || matches!(name.as_ref(), "GLIBC_TUNABLES" | "GCONV_PATH" | "LOCPATH")
        })
        .collect();
    for name in overridden {
        command.env_remove(name);
    }
}
