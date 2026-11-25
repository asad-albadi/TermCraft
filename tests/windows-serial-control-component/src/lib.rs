#![allow(dead_code)]
#[path = "../../../engines/rdp/src/serial_windows_control.rs"]
mod control;
const UNSUPPORTED: u32 = 0xc00000bb;
#[path = "../../../engines/rdp/src/serial_stats.rs"]
mod stats;

include!("completion_helpers.rs");
#[path = "../../../engines/rdp/src/serial_completion.rs"]
mod serial_completion;

#[cfg(test)]
mod completion_integration {
    use super::*;
    fn request() -> Vec<u8> {
        let mut p = vec![114, 68, 82, 73];
        for word in [7u32, 9, 42, 14, 0] {
            p.extend(word.to_le_bytes());
        }
        p
    }
    #[test]
    fn old_query_winning_cancel_keeps_its_origin_after_reopen() {
        assert_eq!(
            stats::completed_count(None, control::GET_STATS, &mut [1; 24], 24),
            Err(UNSUPPORTED)
        );
        let old_origin = [0xfe; 24];
        let next_origin = [0xff; 24];
        let mut old_sample = vec![0xff; 24];
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut old_sample,
            || {},
            || Ok(24),
            |out, transferred| {
                stats::completed_count(Some(&old_origin), control::GET_STATS, out, transferred)
            },
        );
        let mut fresh = [0xff; 24];
        stats::completed_count(Some(&next_origin), control::GET_STATS, &mut fresh, 24).unwrap();
        assert_eq!(&response[12..20], &[0, 0, 0, 0, 24, 0, 0, 0]);
        assert_eq!(&response[20..], &[1; 24]);
        assert_eq!(fresh, [0; 24]);
    }
    #[test]
    fn purge_success_after_cancel_has_no_fabricated_output() {
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut vec![],
            || {},
            || Ok(4),
            |_, transferred| control::completed_count(control::PURGE, transferred),
        );
        assert_eq!(&response[12..], &[0; 8]);
    }
    #[test]
    fn wait_zero_and_real_event_masks_survive_normal_completion_winning_cancel() {
        for mask in [0u32, 1, 0x108] {
            let mut output = mask.to_le_bytes().to_vec();
            let response = serial_completion::cancel_and_collect(
                &request(),
                14,
                &mut output,
                || {},
                || Ok(4),
                |_, transferred| control::completed_count(control::WAIT_ON_MASK, transferred),
            );
            assert_eq!(&response[12..20], &[0, 0, 0, 0, 4, 0, 0, 0]);
            assert_eq!(&response[20..], &mask.to_le_bytes());
        }
    }
    #[test]
    fn aborted_wait_reports_native_failure_without_returning_zero_mask_success() {
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut vec![0; 4],
            || {},
            || Err(0xc0000120),
            |_, _| panic!("failed native wait must not transform output"),
        );
        assert_eq!(&response[12..], &[0x20, 1, 0, 0xc0, 0, 0, 0, 0]);
    }
    #[test]
    fn terminal_native_write_precedes_origin_subtraction() {
        // Native I/O owns this allocation until wait returns. The simulated
        // native write uses only the pointer saved at submission, not a slice
        // supplied to the wait callback.
        let mut output = vec![0; 24];
        let native_output = output.as_mut_ptr();
        let origin = [40u8, 0, 0, 0].repeat(6).try_into().unwrap();
        let sample = [43u8, 0, 0, 0].repeat(6);
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut output,
            || {},
            || {
                unsafe { std::ptr::copy_nonoverlapping(sample.as_ptr(), native_output, 24) };
                Ok(24)
            },
            |out, transferred| {
                stats::completed_count(Some(&origin), control::GET_STATS, out, transferred)
            },
        );
        assert_eq!(&response[12..20], &[0, 0, 0, 0, 24, 0, 0, 0]);
        assert_eq!(&response[20..], &[3u8, 0, 0, 0].repeat(6));
    }
    #[test]
    fn native_failure_preserves_output_and_skips_stats_conversion() {
        let mut output = vec![0xff; 24];
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut output,
            || {},
            || Err(0xc000009d),
            |_, _| panic!("native failure must bypass stats conversion"),
        );
        assert_eq!(&response[12..], &[0x9d, 0, 0, 0xc0, 0, 0, 0, 0]);
        assert_eq!(output, vec![0xff; 24]);
    }
    #[test]
    fn cancellation_then_terminal_wait_then_conversion_happen_once_in_order() {
        let stage = std::cell::Cell::new(0);
        let mut output = vec![7];
        let response = serial_completion::cancel_and_collect(
            &request(),
            14,
            &mut output,
            || {
                assert_eq!(stage.replace(1), 0);
            },
            || {
                assert_eq!(stage.replace(2), 1);
                Ok(1)
            },
            |out, transferred| {
                assert_eq!(stage.replace(3), 2);
                out[0] = 9;
                Ok(transferred)
            },
        );
        assert_eq!(stage.get(), 3);
        assert_eq!(&response[12..], &[0, 0, 0, 0, 1, 0, 0, 0, 9]);
    }
}
