use harbor_workspace::tools::capture::{CaptureConfig, PacketCapture, inspect_pcap};
#[cfg(target_os = "linux")]
use harbor_workspace::tools::capture::{CaptureHandle, StopReason, enumerate_interfaces};
use std::{fs, path::Path, time::Duration};
#[cfg(target_os = "linux")]
use std::{net::UdpSocket, thread, time::Instant};

fn config(path: &Path, filter: &str) -> CaptureConfig {
    CaptureConfig {
        interface: "lo".into(),
        filter: filter.into(),
        output: path.into(),
        snaplen: 128,
        max_duration: Duration::from_secs(3),
        max_bytes: 4096,
        max_packets: 1,
    }
}

#[test]
fn rejects_invalid_configuration_and_pcap() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.pcap");
    let mut limits = config(&output, "udp");
    limits.interface.clear();
    assert!(PacketCapture::start(limits).is_err());
    fs::write(&output, [0; 24]).unwrap();
    assert!(inspect_pcap(&output).is_err());
}

#[test]
fn reads_big_endian_nanosecond_headers_and_rejects_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("big-endian.pcap");
    let mut bytes = vec![0xa1, 0xb2, 0x3c, 0x4d, 0, 2, 0, 4];
    for value in [0u32, 0, 64, 113, 31, 123_456_789, 4, 10] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&[0; 4]);
    fs::write(&output, &bytes).unwrap();
    let metadata = inspect_pcap(&output).unwrap();
    assert_eq!(metadata.linktype, 113);
    assert_eq!(metadata.snaplen, 64);
    assert_eq!(metadata.packets, 1);
    assert_eq!(metadata.first_timestamp.unwrap().nanoseconds, 123_456_789);
    bytes.pop();
    fs::write(&output, &bytes).unwrap();
    assert!(inspect_pcap(&output).is_err());
}

#[cfg(target_os = "linux")]
fn in_isolated_namespace() {
    let parent =
        std::env::var("HARBOR_TEST_PARENT_NETNS").expect("run in an isolated network namespace");
    let current = fs::read_link("/proc/self/ns/net").unwrap();
    assert_ne!(
        current.to_string_lossy(),
        parent,
        "refusing capture outside the isolated namespace"
    );
}

#[cfg(target_os = "linux")]
fn wait_for_progress(handle: &CaptureHandle, packets: u64) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let progress = handle.progress();
        if progress.ready && progress.packets >= packets {
            break;
        }
        assert!(
            Instant::now() < deadline && !handle.is_finished(),
            "Capture did not reach expected progress"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "run in an isolated namespace with CAP_NET_RAW dropped"]
fn reports_kernel_capture_permission_denied() {
    in_isolated_namespace();
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("denied.pcap");
    let error = config(&output, "udp").start().unwrap().wait().unwrap_err();
    assert!(error.contains("permission denied"), "{error}");
    assert!(!output.exists());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "run only in a fresh network namespace with loopback enabled"]
fn captures_only_selected_loopback_packets_and_honors_limits() {
    use std::os::unix::fs::PermissionsExt;

    in_isolated_namespace();
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.pcap");
    assert!(
        enumerate_interfaces()
            .unwrap()
            .iter()
            .any(|item| item.name == "lo")
    );
    let selected = UdpSocket::bind("127.0.0.1:0").unwrap();
    let excluded = UdpSocket::bind("127.0.0.1:0").unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let filter = format!("udp dst port {}", selected.local_addr().unwrap().port());
    let handle = config(&output, &filter).start().unwrap();
    wait_for_progress(&handle, 0);
    sender
        .send_to(b"excluded test payload", excluded.local_addr().unwrap())
        .unwrap();
    sender
        .send_to(b"selected test payload", selected.local_addr().unwrap())
        .unwrap();
    let report = handle.wait().unwrap();
    assert_eq!(report.reason, StopReason::PacketLimit);
    assert_eq!(report.metadata, inspect_pcap(&output).unwrap());
    assert_eq!(report.metadata.packets, 1);
    assert_eq!(report.metadata.linktype, 1);
    assert_eq!(report.metadata.snaplen, 128);
    assert_ne!(report.metadata.first_timestamp.unwrap().seconds, 0);
    let bytes = fs::read(&output).unwrap();
    assert!(bytes.ends_with(b"selected test payload"));
    assert_eq!(report.metadata.bytes, bytes.len() as u64);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let truncated_output = dir.path().join("truncated.pcap");
    let mut truncation = config(&truncated_output, &filter);
    truncation.snaplen = 64;
    let truncation = truncation.start().unwrap();
    wait_for_progress(&truncation, 0);
    sender
        .send_to(&[0x7e; 256], selected.local_addr().unwrap())
        .unwrap();
    let report = truncation.wait().unwrap();
    assert_eq!(report.metadata.snaplen, 64);
    assert_eq!(report.metadata.bytes, 24 + 16 + 64);
    assert!(fs::read(&truncated_output).unwrap().ends_with(&[0x7e; 22]));

    let idle_output = dir.path().join("idle.pcap");
    let idle = config(&idle_output, &filter).start().unwrap();
    wait_for_progress(&idle, 0);
    let cancel_started = Instant::now();
    idle.cancel();
    let report = idle.wait().unwrap();
    assert_eq!(report.reason, StopReason::Cancelled);
    assert_eq!(report.metadata.packets, 0);
    assert!(cancel_started.elapsed() < Duration::from_secs(1));

    let partial_output = dir.path().join("partial.pcap");
    let mut partial_config = config(&partial_output, &filter);
    partial_config.max_packets = 5;
    let partial = partial_config.start().unwrap();
    wait_for_progress(&partial, 0);
    sender
        .send_to(b"partial capture", selected.local_addr().unwrap())
        .unwrap();
    wait_for_progress(&partial, 1);
    partial.cancel();
    assert_eq!(partial.wait().unwrap().metadata.packets, 1);
    assert!(
        fs::read(&partial_output)
            .unwrap()
            .ends_with(b"partial capture")
    );

    let collision_output = dir.path().join("collision.pcap");
    let collision = config(&collision_output, &filter).start().unwrap();
    wait_for_progress(&collision, 0);
    fs::write(&collision_output, b"created during capture").unwrap();
    collision.cancel();
    assert!(collision.wait().is_err());
    assert_eq!(
        fs::read(&collision_output).unwrap(),
        b"created during capture"
    );

    let malformed_output = dir.path().join("malformed.pcap");
    let error = config(&malformed_output, "udp and (((")
        .start()
        .unwrap()
        .wait()
        .unwrap_err();
    assert!(error.contains("filter"), "{error}");
    assert!(!malformed_output.exists());
    assert!(config(&output, &filter).start().unwrap().wait().is_err());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    let symlink = dir.path().join("symlink.pcap");
    std::os::unix::fs::symlink(&malformed_output, &symlink).unwrap();
    assert!(config(&symlink, &filter).start().unwrap().wait().is_err());
    assert!(!malformed_output.exists());

    let byte_output = dir.path().join("byte.pcap");
    let mut byte_limited = config(&byte_output, &filter);
    byte_limited.max_bytes = 50;
    let byte_limited = byte_limited.start().unwrap();
    wait_for_progress(&byte_limited, 0);
    sender
        .send_to(b"too large for output", selected.local_addr().unwrap())
        .unwrap();
    let byte_limited = byte_limited.wait().unwrap();
    assert_eq!(byte_limited.reason, StopReason::ByteLimit);
    assert_eq!(byte_limited.metadata.bytes, 24);

    let timeout_output = dir.path().join("timeout.pcap");
    let mut timeout = config(&timeout_output, &filter);
    timeout.max_duration = Duration::from_millis(30);
    let timed = timeout.start().unwrap().wait().unwrap();
    assert_eq!(timed.reason, StopReason::TimeLimit);
    assert_eq!(timed.metadata.packets, 0);
}
