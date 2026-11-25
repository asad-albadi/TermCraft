#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellFlavor {
    Bash,
    Zsh,
    Fish,
    Unsupported,
}
pub fn integration_script(shell: ShellFlavor, host: &str) -> Result<String, String> {
    if host.is_empty()
        || host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '/' | '\\' | '@'))
    {
        return Err("Choose a valid SSH host before copying shell setup.".into());
    }
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let mut authority = String::new();
    for byte in host.bytes() {
        if byte.is_ascii_alphanumeric() || b"._~-:[]".contains(&byte) {
            authority.push(byte as char);
        } else {
            authority.push_str(&format!("%{byte:02X}"));
        }
    }
    let template = match shell {
        ShellFlavor::Bash => include_str!("bash.sh"),
        ShellFlavor::Zsh => include_str!("zsh.sh"),
        ShellFlavor::Fish => include_str!("fish.fish"),
        ShellFlavor::Unsupported => {
            return Err(
                "Choose Bash, Zsh or Fish for shell setup; manual browsing remains available."
                    .into(),
            );
        }
    };
    Ok(template.replace("@TERMCRAFT_HOST@", &format!("'{authority}'")))
}
