//! Darwin helper ownership is fixed by the spawning process, never by adoption.
use std::{ffi::OsStr, time::Duration};

pub(super) fn expected_owner(argument: Option<&OsStr>) -> Result<libc::pid_t, String> {
    argument
        .and_then(OsStr::to_str)
        .and_then(|value| value.parse::<libc::pid_t>().ok())
        .filter(|owner| *owner > 0)
        .ok_or_else(|| "Missing or invalid native printer owner identity".into())
}

fn validate(expected: libc::pid_t, actual: libc::pid_t) -> Result<(), String> {
    if expected <= 0 || actual != expected {
        return Err("The native printer owner exited before helper initialization".into());
    }
    Ok(())
}

pub(super) fn watch(expected: libc::pid_t) -> Result<(), String> {
    // This check precedes INIT decoding and any Backend::open/native access.
    validate(expected, unsafe { libc::getppid() })?;
    std::thread::Builder::new()
        .name("printer-owner-watch".into())
        .spawn(move || {
            loop {
                // Keep the spawn-time identity even if the owner dies between the
                // initial validation and this thread's first scheduling opportunity.
                if unsafe { libc::getppid() } != expected {
                    unsafe { libc::_exit(1) }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .map_err(|_| "Cannot watch native printer ownership")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_identity_must_be_explicit_and_positive() {
        assert!(expected_owner(None).is_err());
        for value in ["", "0", "-1", "2147483648", "owner"] {
            assert!(expected_owner(Some(OsStr::new(value))).is_err());
        }
        assert_eq!(expected_owner(Some(OsStr::new("1234"))).unwrap(), 1234);
    }
    #[test]
    fn an_adopted_parent_is_not_the_known_owner() {
        assert!(validate(1234, 5678).is_err());
        assert!(validate(0, 0).is_err());
        assert!(validate(1234, 1234).is_ok());
    }
}
