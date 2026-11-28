use super::*;
use harbor_workspace::remote_files::{EntryKind, RemoteEntry, RemotePath};

pub(super) fn remote_samples() -> Vec<RemoteEntry> {
    [
        ("folder.png", EntryKind::Directory),
        ("photo.PNG", EntryKind::File),
        ("sample.unknown", EntryKind::File),
        ("link.sh", EntryKind::Symlink),
    ]
    .into_iter()
    .map(|(name, kind)| RemoteEntry {
        path: RemotePath::from_bytes(format!("/{name}").into_bytes()).unwrap(),
        display_name: name.into(),
        kind,
        size: Some(123),
    })
    .collect()
}

pub(super) const REMOTE_LABELS: &[(&str, &str)] = &[
    ("folder.png", "Folder"),
    ("photo.PNG", "Image"),
    ("sample.unknown", "File"),
    ("link.sh", "Symbolic link"),
];

/// Inspect actual icon shapes relative to the filename. Discard color so that
/// color alone cannot distinguish two file types.
pub(super) fn assert_distinct_icons(output: &FullOutput, files: &[(&str, &str)]) {
    let mut outlines = std::collections::BTreeSet::new();
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    for &(name, kind) in files {
        let label = format!("{name} · {kind}");
        assert!(
            nodes
                .iter()
                .any(|(_, node)| node.label() == Some(label.as_str())),
            "missing accessible row {label}"
        );
        let text = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(t)
                    if t.galley.text() == name
                        && s.clip_rect.contains(t.pos + t.galley.size() * 0.5) =>
                {
                    Some(t)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible filename {name}"));
        let left = text.pos.x;
        let y = text.pos.y + text.galley.size().y / 2.;
        let area = Rect::from_min_max(pos2(left - 36., y - 13.), pos2(left - 2., y + 13.));
        let mut shapes = Vec::new();
        for clipped in &output.shapes {
            let bounds = clipped.shape.visual_bounding_rect();
            if bounds.is_positive()
                && area.contains_rect(bounds)
                && clipped.clip_rect.contains_rect(bounds)
            {
                let bounds = bounds.translate(vec2(-left, -y));
                shapes.push(
                    [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()]
                        .map(|v| (v * 4.).round() as i32),
                );
            }
        }
        assert!(
            !shapes.is_empty(),
            "{name} has no visible icon before its filename"
        );
        assert!(
            outlines.insert(shapes),
            "{label} has the same geometry as another type"
        );
    }
}

#[test]
fn local_explorer_draws_distinct_file_type_shapes_without_covering_names() {
    let (ctx, root, mut workspace) = super::terminal_review_tests::owned_workspace();
    let dir = root.path().join("local-icons");
    fs::create_dir(&dir).unwrap();
    let files = [
        #[cfg(unix)]
        ("photo-link.sh", "Symbolic link"),
        ("reports.png", "Folder"),
        ("main.RS", "Source code"),
        (".editorconfig", "Configuration"),
        ("README", "Text document"),
        ("report.docx", "Document"),
        ("guide.PDF", "PDF document"),
        ("picture.PNG", "Image"),
        ("track.flac", "Audio"),
        ("clip.mp4", "Video"),
        ("backup.TAR.GZ", "Archive"),
        ("install.ps1", "Script"),
        ("program.exe", "Executable"),
        ("file.unknown", "File"),
    ];
    for (name, kind) in files {
        #[cfg(unix)]
        if kind == "Symbolic link" {
            std::os::unix::fs::symlink("picture.PNG", dir.join(name)).unwrap();
            continue;
        }
        if kind == "Folder" {
            fs::create_dir(dir.join(name)).unwrap()
        } else {
            fs::write(dir.join(name), b"a").unwrap()
        }
    }
    workspace.files.path = dir;
    workspace.files.refresh();
    ctx.enable_accesskit();
    let draw = |w: &mut Workspace| {
        super::gui_tests::shell_frame(&ctx, vec2(760., 1350.), 1., vec![], |ui| w.files_page(ui))
    };
    draw(&mut workspace);
    assert_distinct_icons(&draw(&mut workspace), &files);
}

#[test]
fn ssh_listing_draws_icons_for_names_types_and_raw_links() {
    let (ctx, _root, mut w) = super::terminal_review_tests::owned_workspace();
    w.start(Session {
        host: "127.0.0.1".into(),
        port: 1,
        ..Default::default()
    });
    let tab = &mut w.tabs[0];
    drop(tab.ssh.take());
    tab.ssh_connected = true;
    let listing = tab.browser.as_mut().unwrap();
    listing.loading = false;
    listing.entries = remote_samples();
    ctx.enable_accesskit();
    let draw = |w: &mut Workspace| {
        super::gui_tests::shell_frame(&ctx, vec2(1360., 1800.), 1., vec![], |ui| {
            w.remote_files_panel(ui)
        })
    };
    draw(&mut w);
    assert_distinct_icons(&draw(&mut w), REMOTE_LABELS);
}
