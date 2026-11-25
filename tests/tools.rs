use harbor_workspace::tools::{
    Change, FileDiff, FolderDiff, ImagePreview, PortScan, PortStatus, ScanTarget,
};
use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    path::PathBuf,
};

#[test]
fn folder_diff_reports_added_removed_changed() {
    let fixture = tempfile::tempdir().unwrap();
    let left = fixture.path().join("left");
    let right = fixture.path().join("right");
    fs::create_dir(&left).unwrap();
    fs::create_dir(&right).unwrap();
    fs::write(left.join("old.txt"), "old").unwrap();
    fs::write(right.join("new.txt"), "new").unwrap();
    fs::write(left.join("changed.txt"), "1").unwrap();
    fs::write(right.join("changed.txt"), "2").unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();
    let diff = FolderDiff::compare(&left, &right).unwrap();
    assert_eq!(diff.added, vec![PathBuf::from("new.txt")]);
    assert_eq!(diff.removed, vec![PathBuf::from("old.txt")]);
    assert_eq!(diff.changed, vec![PathBuf::from("changed.txt")]);
}

#[test]
fn file_diff_reports_utf8_insertion_and_distinguishes_binary() {
    let fixture = tempfile::tempdir().unwrap();
    let left = fixture.path().join("left");
    let right = fixture.path().join("right");
    fs::write(&left, "café\nlast\n").unwrap();
    fs::write(&right, "café\n追加\nlast\n").unwrap();
    assert_eq!(
        FileDiff::compare(&left, &right).unwrap().changes,
        Some(vec![Change::Added("追加\n".into())])
    );
    fs::write(&left, [0, 0xff, 1]).unwrap();
    fs::write(&right, [0, 0xff, 2]).unwrap();
    let result = FileDiff::compare(&left, &right).unwrap();
    assert!(!result.equal);
    assert_eq!(result.changes, None);
}

#[cfg(unix)]
#[test]
fn folder_diff_rejects_symlinks_without_following_them() {
    let fixture = tempfile::tempdir().unwrap();
    let folder = fixture.path().join("folder");
    fs::create_dir(&folder).unwrap();
    std::os::unix::fs::symlink(&folder, folder.join("loop")).unwrap();
    assert!(
        FolderDiff::compare(&folder, &folder)
            .unwrap_err()
            .contains("symbolic link")
    );
}

#[test]
fn folder_diff_enforces_depth_limit() {
    let fixture = tempfile::tempdir().unwrap();
    let mut path = fixture.path().to_owned();
    for _ in 0..40 {
        path.push("d");
        fs::create_dir(&path).unwrap();
    }
    assert!(
        FolderDiff::compare(fixture.path(), fixture.path())
            .unwrap_err()
            .contains("depth")
    );
}

#[test]
fn image_preview_decodes_known_pixels() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("pixels.png");
    image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 128, 255, 64])
        .unwrap()
        .save(&path)
        .unwrap();
    let preview = ImagePreview::open(&path, 2).unwrap();
    assert_eq!((preview.width, preview.height), (2, 1));
    assert_eq!(preview.rgba, [255, 0, 0, 255, 0, 128, 255, 64]);
}

#[test]
fn oversized_image_is_rejected_before_allocation() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("pixels.png");
    image::RgbaImage::new(4, 4).save(&path).unwrap();
    let mut encoded = fs::read(&path).unwrap();
    let pos = encoded
        .windows(4)
        .position(|window| window == b"IDAT")
        .unwrap()
        + 4;
    encoded[pos] ^= 0xff;
    fs::write(&path, encoded).unwrap();
    assert!(
        ImagePreview::open(&path, 15)
            .unwrap_err()
            .contains("pixel limit")
    );
    assert!(ImagePreview::open(&path, 16).is_err());
}

#[test]
fn scan_reports_configured_loopback_ports() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let open = listener.local_addr().unwrap().port();
    let closed = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let report = PortScan::start(ScanTarget {
        address: Ipv4Addr::LOCALHOST.into(),
        ports: vec![open, closed_port, open],
    })
    .unwrap()
    .wait()
    .unwrap();
    assert_eq!(report.address, Ipv4Addr::LOCALHOST);
    assert_eq!(report.ports.len(), 2);
    assert_eq!(report.ports[&open], PortStatus::Open);
    assert_eq!(report.ports[&closed_port], PortStatus::Closed);
    assert!(!report.cancelled);
}

#[test]
fn scan_cancel_joins_worker() {
    let handle = PortScan::start(ScanTarget {
        address: Ipv4Addr::LOCALHOST.into(),
        ports: (60000..60256).collect(),
    })
    .unwrap();
    handle.cancel();
    let report = handle.wait().unwrap();
    assert!(report.cancelled);
}

#[test]
fn scan_rejects_wildcard_addresses_and_invalid_port_lists() {
    for target in [
        ScanTarget {
            address: Ipv4Addr::UNSPECIFIED.into(),
            ports: vec![80],
        },
        ScanTarget {
            address: "::ffff:255.255.255.255".parse().unwrap(),
            ports: vec![80],
        },
        ScanTarget {
            address: "ff02::1".parse().unwrap(),
            ports: vec![80],
        },
        ScanTarget {
            address: Ipv4Addr::LOCALHOST.into(),
            ports: vec![0],
        },
        ScanTarget {
            address: Ipv4Addr::LOCALHOST.into(),
            ports: (1..258).collect(),
        },
    ] {
        assert!(target.validate().is_err());
    }
}

#[test]
fn image_preview_decodes_jpeg_pixels() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("pixels.jpg");
    image::RgbImage::from_pixel(8, 8, image::Rgb([48, 96, 192]))
        .save(&path)
        .unwrap();
    let preview = ImagePreview::open(&path, 64).unwrap();
    assert_eq!((preview.width, preview.height), (8, 8));
    assert_eq!(preview.rgba.len(), 256);
    for pixel in preview.rgba.chunks_exact(4) {
        assert!(pixel[0].abs_diff(48) <= 3);
        assert!(pixel[1].abs_diff(96) <= 3);
        assert!(pixel[2].abs_diff(192) <= 3);
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn file_diff_rejects_oversized_input() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("large");
    fs::File::create(&path)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        FileDiff::compare(&path, &path)
            .unwrap_err()
            .contains("byte limit")
    );
}

#[test]
fn port_check_configuration_accepts_ipv4_and_ipv6_without_connecting() {
    for address in ["192.0.2.1", "2001:db8::1"] {
        let target = ScanTarget {
            address: address.parse().unwrap(),
            ports: vec![443],
        };
        assert!(target.validate().is_ok());
    }
}

#[test]
fn folder_comparison_checks_cancellation() {
    let fixture = tempfile::tempdir().unwrap();
    fs::write(fixture.path().join("file"), "fixture").unwrap();
    let checks = std::cell::Cell::new(0);
    let result = FolderDiff::compare_with_cancel(fixture.path(), fixture.path(), &|| {
        checks.set(checks.get() + 1);
        checks.get() >= 3
    });
    assert_eq!(checks.get(), 3);
    assert!(result.unwrap_err().contains("cancelled"));
}
