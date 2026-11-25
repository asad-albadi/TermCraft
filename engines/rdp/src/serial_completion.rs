//! Native serial completion encoding, shared with bounded non-device tests.
use super::{INVALID, complete, length_data};

pub(super) fn terminal_data(
    major: u32,
    output: &mut Vec<u8>,
    outcome: Result<u32, u32>,
) -> Result<Vec<u8>, u32> {
    let transferred = outcome?;
    if major == 4 {
        let mut count = transferred.to_le_bytes().to_vec();
        count.push(0);
        Ok(count)
    } else if transferred as usize > output.len() {
        Err(INVALID)
    } else {
        output.truncate(transferred as usize);
        Ok(length_data(std::mem::take(output)))
    }
}

pub(super) fn cancel_and_collect(
    request: &[u8],
    major: u32,
    output: &mut Vec<u8>,
    request_cancel: impl FnOnce(),
    wait_terminal: impl FnOnce() -> Result<u32, u32>,
    transform_output: impl FnOnce(&mut [u8], u32) -> Result<u32, u32>,
) -> Vec<u8> {
    request_cancel();
    // Do not borrow the allocation as a slice while native I/O may still
    // write it. Only terminal success permits output inspection/conversion.
    let outcome = wait_terminal();
    let outcome = outcome.and_then(|transferred| transform_output(output, transferred));
    complete(request, terminal_data(major, output, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(major: u32) -> Vec<u8> {
        let mut request = vec![114, 68, 82, 73];
        for word in [7u32, 9, 42, major, 0] {
            request.extend(word.to_le_bytes());
        }
        request
    }
    #[test]
    fn close_reports_write_success_when_normal_completion_wins_cancellation() {
        let response = cancel_and_collect(
            &request(4),
            4,
            &mut Vec::new(),
            || {},
            || Ok(3),
            |_, n| Ok(n),
        );
        assert_eq!(
            response,
            [
                114, 68, 67, 73, 7, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0
            ]
        );
    }
    #[test]
    fn close_preserves_terminal_abort_and_other_native_failure() {
        for code in [0xc0000120u32, 0xc000009d] {
            let response = cancel_and_collect(
                &request(4),
                4,
                &mut Vec::new(),
                || {},
                || Err(code),
                |_, _| panic!("failed wait must not inspect output"),
            );
            assert_eq!(
                u32::from_le_bytes(response[12..16].try_into().unwrap()),
                code
            );
            assert_eq!(&response[16..], &[0; 5]);
        }
    }
    #[test]
    fn close_forwards_exact_terminal_read_and_control_output() {
        let read = cancel_and_collect(
            &request(3),
            3,
            &mut vec![b'a', b'b', b'c'],
            || {},
            || Ok(2),
            |_, n| Ok(n),
        );
        assert_eq!(&read[12..], &[0, 0, 0, 0, 2, 0, 0, 0, b'a', b'b']);
        let control = cancel_and_collect(
            &request(14),
            14,
            &mut Vec::new(),
            || {},
            || Ok(0),
            |_, n| Ok(n),
        );
        assert_eq!(&control[12..], &[0; 8]);
    }
}
