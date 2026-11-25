//! Fixed worker argument, with a quoted native UTF-16 executable path.
pub(super) fn command_line(executable: &[u16]) -> Result<Vec<u16>, ()> {
    if executable.is_empty() || executable.iter().any(|unit| [0, 34].contains(unit)) {
        return Err(());
    }
    let mut out = vec![34];
    out.extend_from_slice(executable);
    out.extend([34, 32]);
    out.extend("--pcsc-worker".encode_utf16());
    out.push(0);
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_unicode_helper_path_without_literal_backslashes() {
        let path: Vec<u16> = r"C:\Program Files\日本語\worker.exe"
            .encode_utf16()
            .collect();
        let command = command_line(&path).unwrap();
        let expected: Vec<u16> =
            format!("\"{}\" --pcsc-worker\0", String::from_utf16(&path).unwrap())
                .encode_utf16()
                .collect();
        assert_eq!(command, expected);
        assert!(command_line(&[65, 0, 66]).is_err());
        assert!(command_line(&[34]).is_err());
    }
}
