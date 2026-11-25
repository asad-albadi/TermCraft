//! Windows serial IOCTL validation, executable without a serial device.
const INVALID: u32 = 0xc000000d;
const DENIED: u32 = 0xc0000022;
const UNSUPPORTED: u32 = 0xc00000bb;
const BUFFER_TOO_SMALL: u32 = 0xc0000023;
const MAX_IO: usize = 64 * 1024;
pub(super) const GET_WAIT_MASK: u32 = 0x001b0040;
pub(super) const SET_WAIT_MASK: u32 = 0x001b0044;
pub(super) const WAIT_ON_MASK: u32 = 0x001b0048;
pub(super) const PURGE: u32 = 0x001b004c;
pub(super) const GET_COMMSTATUS: u32 = 0x001b006c;
pub(super) const GET_PROPERTIES: u32 = 0x001b0074;
pub(super) const GET_STATS: u32 = 0x001b008c;
pub(super) const STATUS_SIZE: usize = 20;
pub(super) const PROPERTIES_SIZE: usize = 64;
pub(super) const STATS_SIZE: usize = 24;
#[derive(Debug, PartialEq)]
pub(super) struct Control {
    pub code: u32,
    pub output_len: usize,
    pub wait: bool,
}
fn word(body: &[u8], offset: usize) -> Result<u32, u32> {
    Ok(u32::from_le_bytes(
        body.get(offset..offset + 4)
            .ok_or(INVALID)?
            .try_into()
            .unwrap(),
    ))
}
// Classify wire shape only. The device worker still checks live handle identity,
// real access rights, and native driver support before this request can run.
pub(crate) fn is_wait_mask_set(body: &[u8]) -> bool {
    word(body, 8) == Ok(SET_WAIT_MASK) && validate(body, true, true, false).is_ok()
}

pub(super) fn validate(
    body: &[u8],
    reads: bool,
    writes: bool,
    wait_pending: bool,
) -> Result<Control, u32> {
    let capacity = word(body, 0)? as usize;
    let len = word(body, 4)? as usize;
    let code = word(body, 8)?;
    if capacity > MAX_IO || len > MAX_IO || body.len() != 32 + len {
        return Err(INVALID);
    }
    // All codes are explicitly named by MS-RDPESP 2.2.2.6. No arbitrary
    // DeviceIoControl value, driver namespace, or buffer size comes from the peer.
    let (input_len, output_len, query) = match code {
        0x001b0004 => (4, 0, false),
        0x001b0050 => (0, 4, true),
        0x001b000c => (3, 0, false),
        0x001b0054 => (0, 3, true),
        0x001b001c => (20, 0, false),
        0x001b0020 => (0, 20, true),
        0x001b0058 => (6, 0, false),
        0x001b005c => (0, 6, true),
        0x001b0064 => (16, 0, false),
        0x001b0060 => (0, 16, true),
        0x001b0068 | 0x001b0078 | GET_WAIT_MASK | WAIT_ON_MASK => (0, 4, true),
        SET_WAIT_MASK | PURGE => (4, 0, false),
        // GET_COMMSTATUS acknowledges (clears) accumulated driver errors.
        GET_COMMSTATUS => (0, STATUS_SIZE, false),
        GET_PROPERTIES => (0, PROPERTIES_SIZE, true),
        GET_STATS => (0, STATS_SIZE, true),
        0x001b0024 | 0x001b0028 | 0x001b0030 | 0x001b0034 | 0x001b0038 | 0x001b003c
        | 0x001b0010 | 0x001b0014 => (0, 0, false),
        _ => return Err(UNSUPPORTED),
    };
    if len != input_len {
        return Err(INVALID);
    }
    if capacity < output_len {
        return Err(BUFFER_TOO_SMALL);
    }
    if !query && !writes {
        return Err(DENIED);
    }
    if code == WAIT_ON_MASK {
        if !reads {
            return Err(DENIED);
        }
        // The Windows driver permits one pending wait per opened port. Keep
        // the same bound before issuing another native request.
        if wait_pending {
            return Err(INVALID);
        }
    }
    if code == SET_WAIT_MASK && word(body, 32)? & !0x1fff != 0 {
        return Err(INVALID);
    }
    if code == PURGE {
        let mask = word(body, 32)?;
        if mask == 0 || mask & !0x0f != 0 {
            return Err(INVALID);
        }
    }
    Ok(Control {
        code,
        output_len: if code == GET_PROPERTIES {
            capacity
        } else {
            output_len
        },
        wait: code == WAIT_ON_MASK,
    })
}
pub(super) fn completed_count(code: u32, count: u32) -> Result<u32, u32> {
    // A purge has no output buffer. ntddser permits Information=sizeof(ULONG)
    // for its consumed input mask. It is not four returned output bytes.
    if code == PURGE {
        return if count == 0 || count == 4 {
            Ok(0)
        } else {
            Err(INVALID)
        };
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(code: u32, input: &[u8], capacity: usize) -> Vec<u8> {
        let mut b = vec![0; 32];
        b[0..4].copy_from_slice(&(capacity as u32).to_le_bytes());
        b[4..8].copy_from_slice(&(input.len() as u32).to_le_bytes());
        b[8..12].copy_from_slice(&code.to_le_bytes());
        b.extend(input);
        b
    }
    #[test]
    fn fixed_native_getters_use_exact_output_struct_sizes() {
        for (code, size) in [
            (GET_WAIT_MASK, 4),
            (GET_COMMSTATUS, STATUS_SIZE),
            (GET_STATS, STATS_SIZE),
        ] {
            assert_eq!(
                validate(&body(code, &[], size), true, true, false)
                    .unwrap()
                    .output_len,
                size
            );
            assert_eq!(
                validate(&body(code, &[], size - 1), true, true, false),
                Err(BUFFER_TOO_SMALL)
            );
            assert_eq!(
                validate(&body(code, &[0], size), true, true, false),
                Err(INVALID)
            );
        }
    }
    #[test]
    fn properties_preserve_room_for_driver_provider_extension() {
        assert_eq!(
            validate(&body(GET_PROPERTIES, &[], 128), true, false, false)
                .unwrap()
                .output_len,
            128
        );
        assert_eq!(
            validate(&body(GET_PROPERTIES, &[], 63), true, false, false),
            Err(BUFFER_TOO_SMALL)
        );
    }
    #[test]
    fn purge_requires_nonzero_known_bits_and_write_authority() {
        for mask in 1u32..16 {
            assert_eq!(
                validate(&body(PURGE, &mask.to_le_bytes(), 0), false, true, false)
                    .unwrap()
                    .output_len,
                0
            );
            assert_eq!(
                validate(&body(PURGE, &mask.to_le_bytes(), 0), true, false, false),
                Err(DENIED)
            );
        }
        for mask in [0u32, 16, 0xffffffff] {
            assert_eq!(
                validate(&body(PURGE, &mask.to_le_bytes(), 0), true, true, false),
                Err(INVALID)
            );
        }
    }
    #[test]
    fn wait_masks_accept_defined_flags_and_zero_but_no_reserved_bits() {
        for mask in [0u32, 1, 0x1fff] {
            assert!(
                !validate(
                    &body(SET_WAIT_MASK, &mask.to_le_bytes(), 0),
                    true,
                    true,
                    true
                )
                .unwrap()
                .wait
            );
        }
        assert_eq!(
            validate(
                &body(SET_WAIT_MASK, &0x2000u32.to_le_bytes(), 0),
                true,
                true,
                false
            ),
            Err(INVALID)
        );
        assert_eq!(
            validate(
                &body(SET_WAIT_MASK, &1u32.to_le_bytes(), 0),
                true,
                false,
                false
            ),
            Err(DENIED)
        );
    }
    #[test]
    fn only_one_wait_with_read_authority_and_exact_ulong_output() {
        let b = body(WAIT_ON_MASK, &[], 4);
        assert!(validate(&b, true, false, false).unwrap().wait);
        assert_eq!(validate(&b, true, true, true), Err(INVALID));
        assert_eq!(validate(&b, false, true, false), Err(DENIED));
        assert_eq!(
            validate(&body(WAIT_ON_MASK, &[], 3), true, true, false),
            Err(BUFFER_TOO_SMALL)
        );
    }
    #[test]
    fn error_acknowledgement_is_state_changing_but_stats_properties_masks_are_queries() {
        assert_eq!(
            validate(&body(GET_COMMSTATUS, &[], 20), true, false, false),
            Err(DENIED)
        );
        for (code, size) in [(GET_STATS, 24), (GET_PROPERTIES, 64), (GET_WAIT_MASK, 4)] {
            assert!(validate(&body(code, &[], size), false, false, false).is_ok());
        }
    }
    #[test]
    fn malformed_and_unknown_requests_never_reach_driver() {
        for n in 0..32 {
            assert_eq!(validate(&vec![0; n], true, true, false), Err(INVALID));
        }
        let mut b = body(PURGE, &1u32.to_le_bytes(), 0);
        b.push(0);
        assert_eq!(validate(&b, true, true, false), Err(INVALID));
        assert_eq!(
            validate(&body(GET_STATS, &[], MAX_IO + 1), true, true, false),
            Err(INVALID)
        );
        for code in [0, 0x001b0090, 0x001b0098, 0xffffffff] {
            assert_eq!(
                validate(&body(code, &[], 64), true, true, false),
                Err(UNSUPPORTED)
            );
        }
    }
    #[test]
    fn native_purge_information_is_not_output_bytes() {
        for count in [0, 4] {
            assert_eq!(completed_count(PURGE, count), Ok(0));
        }
        assert_eq!(completed_count(PURGE, 5), Err(INVALID));
        assert_eq!(completed_count(WAIT_ON_MASK, 4), Ok(4));
    }
    #[test]
    fn existing_allowlist_sizes_and_authority_are_preserved() {
        for (code, input, output, query) in [
            (0x001b0004, 4, 0, false),
            (0x001b0050, 0, 4, true),
            (0x001b000c, 3, 0, false),
            (0x001b0054, 0, 3, true),
            (0x001b001c, 20, 0, false),
            (0x001b0020, 0, 20, true),
            (0x001b0058, 6, 0, false),
            (0x001b005c, 0, 6, true),
            (0x001b0064, 16, 0, false),
            (0x001b0060, 0, 16, true),
            (0x001b0068, 0, 4, true),
            (0x001b0078, 0, 4, true),
            (0x001b0024, 0, 0, false),
            (0x001b0028, 0, 0, false),
            (0x001b0030, 0, 0, false),
            (0x001b0034, 0, 0, false),
            (0x001b0038, 0, 0, false),
            (0x001b003c, 0, 0, false),
            (0x001b0010, 0, 0, false),
            (0x001b0014, 0, 0, false),
        ] {
            assert_eq!(
                validate(&body(code, &vec![0; input], output), true, true, false)
                    .unwrap()
                    .output_len,
                output
            );
            assert_eq!(
                validate(&body(code, &vec![0; input], output), true, false, false).is_ok(),
                query
            );
        }
    }
}
