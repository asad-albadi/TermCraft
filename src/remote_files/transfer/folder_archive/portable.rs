//! Directory-capability encoder, shared by Windows and portable filesystem tests.
use super::{Budget, check, error, header, link};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt};
use cap_std::fs::{Dir, Metadata};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
fn same(before: &Metadata, after: &Metadata) -> bool {
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.file_type() == after.file_type()
}
fn mode(info: &Metadata) -> u32 {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        info.mode() & 0o777
    }
    #[cfg(windows)]
    {
        if info.is_dir() {
            0o755
        } else if info.permissions().readonly() {
            0o444
        } else {
            0o644
        }
    }
}
fn filename(name: &std::ffi::OsStr) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(name.as_bytes().to_vec())
    }
    #[cfg(windows)]
    {
        Ok(name
            .to_str()
            .ok_or("Source filename cannot be represented by the remote helper")?
            .as_bytes()
            .to_vec())
    }
}
fn target(path: PathBuf) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(path.as_os_str().as_bytes().to_vec())
    }
    #[cfg(windows)]
    {
        if path.components().any(|p| {
            matches!(
                p,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            )
        }) {
            return Err("Source link must stay inside the selected tree".into());
        }
        Ok(path
            .to_str()
            .ok_or("Source link cannot be represented by the remote helper")?
            .replace('\\', "/")
            .into_bytes())
    }
}
fn walk(
    root: &Dir,
    prefix: &[u8],
    out: &mut File,
    budget: &mut Budget,
    cancel: &tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    for entry in root.entries().map_err(error)? {
        check(cancel)?;
        let entry = entry.map_err(error)?;
        let name = entry.file_name();
        let mut path = prefix.to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend(filename(&name)?);
        budget.add(&path)?;
        let before = root.symlink_metadata(&name).map_err(error)?;
        if before.is_symlink() {
            let value = target(root.read_link_contents(&name).map_err(error)?)?;
            link(&value)?;
            let after = root.symlink_metadata(&name).map_err(error)?;
            if !same(&before, &after)
                || before.modified().map_err(error)? != after.modified().map_err(error)?
            {
                return Err("Source link changed during folder preparation".into());
            }
            header(out, 3, 0o777, &path, value.len() as u64)?;
            out.write_all(&value).map_err(error)?;
        } else if before.is_dir() {
            let child = root.open_dir_nofollow(&name).map_err(error)?;
            let opened = child.dir_metadata().map_err(error)?;
            if !same(&before, &opened) {
                return Err("Source directory changed during folder preparation".into());
            }
            header(out, 1, mode(&opened), &path, 0)?;
            walk(&child, &path, out, budget, cancel)?;
        } else if before.is_file() {
            let mut options = cap_std::fs::OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let mut source = root.open_with(&name, &options).map_err(error)?;
            let opened = source.metadata().map_err(error)?;
            if !same(&before, &opened) || !opened.is_file() {
                return Err("Source file changed during folder preparation".into());
            }
            header(out, 2, mode(&opened), &path, opened.len())?;
            let mut remaining = opened.len();
            let mut buffer = [0u8; 32768];
            while remaining > 0 {
                check(cancel)?;
                let n = remaining.min(buffer.len() as u64) as usize;
                source.read_exact(&mut buffer[..n]).map_err(error)?;
                out.write_all(&buffer[..n]).map_err(error)?;
                remaining -= n as u64;
            }
            if source.read(&mut buffer[..1]).map_err(error)? != 0 {
                return Err("Folder source exceeds its advertised size".into());
            }
            let after = source.metadata().map_err(error)?;
            if !same(&opened, &after)
                || opened.len() != after.len()
                || opened.modified().map_err(error)? != after.modified().map_err(error)?
                || mode(&opened) != mode(&after)
            {
                return Err("Folder source changed while reading".into());
            }
        } else {
            return Err("Folder contains an unsupported special entry".into());
        }
    }
    Ok(())
}
pub(crate) async fn build(
    path: PathBuf,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<tokio::fs::File, String> {
    tokio::task::spawn_blocking(move || {
        check(&cancel)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = path.file_name().ok_or("Choose a folder with a name")?;
        let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority()).map_err(error)?;
        let root = parent.open_dir_nofollow(name).map_err(error)?;
        let mut archive = tempfile::tempfile().map_err(error)?;
        archive.write_all(b"TCAR1\0").map_err(error)?;
        archive
            .write_all(&mode(&root.dir_metadata().map_err(error)?).to_be_bytes())
            .map_err(error)?;
        walk(&root, b"", &mut archive, &mut Budget::default(), &cancel)?;
        archive.write_all(&[0]).map_err(error)?;
        archive.sync_all().map_err(error)?;
        archive.seek(SeekFrom::Start(0)).map_err(error)?;
        Ok(tokio::fs::File::from_std(archive))
    })
    .await
    .map_err(error)?
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    #[tokio::test]
    async fn capability_folder_encoder_preserves_bytes_links_names_and_ordinary_modes() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir_all(source.join("empty")).unwrap();
        std::fs::create_dir(&destination).unwrap();
        let raw = std::ffi::OsString::from_vec(b"literal-\xff-\n-'$()".to_vec());
        let bytes = (0..100_000).map(|i| (i % 256) as u8).collect::<Vec<_>>();
        std::fs::write(source.join(&raw), &bytes).unwrap();
        std::fs::set_permissions(source.join(&raw), std::fs::Permissions::from_mode(0o444))
            .unwrap();
        std::os::unix::fs::symlink("cycle", source.join("cycle")).unwrap();
        let (_sender, cancelled) = tokio::sync::watch::channel(false);
        let archive = super::build(source, cancelled.clone()).await.unwrap();
        let modes = super::super::extract(
            archive.into_std().await,
            std::fs::File::open(&destination).unwrap(),
            cancelled,
        )
        .await
        .unwrap();
        super::super::finalize(std::fs::File::open(&destination).unwrap(), modes)
            .await
            .unwrap();
        assert_eq!(std::fs::read(destination.join(&raw)).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(destination.join(raw))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o444
        );
        assert!(destination.join("empty").is_dir());
        assert_eq!(
            std::fs::read_link(destination.join("cycle")).unwrap(),
            std::path::Path::new("cycle")
        );
    }
    #[tokio::test]
    async fn capability_folder_encoder_rejects_an_unselected_root_and_escaping_link() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("outside")).unwrap();
        std::fs::write(root.path().join("outside/keep"), b"keep").unwrap();
        std::os::unix::fs::symlink("outside", root.path().join("alias")).unwrap();
        let (_sender, cancelled) = tokio::sync::watch::channel(false);
        assert!(
            super::build(root.path().join("alias"), cancelled.clone())
                .await
                .is_err()
        );
        std::fs::create_dir(root.path().join("source")).unwrap();
        std::os::unix::fs::symlink("../outside/keep", root.path().join("source/escape")).unwrap();
        assert!(
            super::build(root.path().join("source"), cancelled)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(root.path().join("outside/keep")).unwrap(),
            b"keep"
        );
    }
}

#[cfg(all(test, unix))]
mod windows_decoder_tests {
    use super::*;
    #[tokio::test]
    async fn portable_decoder_preserves_content_and_resolves_selected_tree_link_types() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir_all(source.join("child")).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(source.join("child/file.txt"), b"decoded").unwrap();
        std::os::unix::fs::symlink("child", source.join("dir-link")).unwrap();
        std::os::unix::fs::symlink("child/file.txt", source.join("file-link")).unwrap();
        let (_sender, cancelled) = tokio::sync::watch::channel(false);
        let archive = build(source, cancelled.clone()).await.unwrap();
        extract(
            archive.into_std().await,
            std::fs::File::open(&destination).unwrap(),
            cancelled,
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(destination.join("file-link")).unwrap(),
            b"decoded"
        );
        assert!(destination.join("dir-link").is_dir());
        assert_eq!(
            std::fs::read_link(destination.join("dir-link")).unwrap(),
            Path::new("child")
        );
    }
    #[tokio::test]
    async fn windows_decoder_rejects_device_names_streams_and_unknown_link_types() {
        for name in ["CON.txt", "file:stream", "a\\b", "tail.", "LPT².txt"] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            let target = root.path().join("target");
            std::fs::create_dir(&source).unwrap();
            std::fs::create_dir(&target).unwrap();
            std::fs::write(source.join(name), b"payload").unwrap();
            let (_sender, cancelled) = tokio::sync::watch::channel(false);
            let archive = build(source, cancelled.clone()).await.unwrap();
            assert!(
                extract(
                    archive.into_std().await,
                    std::fs::File::open(&target).unwrap(),
                    cancelled
                )
                .await
                .is_err(),
                "Accepted {name}"
            );
            assert_eq!(std::fs::read_dir(target).unwrap().count(), 0);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("source")).unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::os::unix::fs::symlink("cycle", root.path().join("source/cycle")).unwrap();
        let (_sender, cancelled) = tokio::sync::watch::channel(false);
        let archive = build(root.path().join("source"), cancelled.clone())
            .await
            .unwrap();
        assert!(
            extract(
                archive.into_std().await,
                std::fs::File::open(root.path().join("target")).unwrap(),
                cancelled
            )
            .await
            .is_err()
        );
    }
}

fn local_component(bytes: &[u8]) -> Result<std::ffi::OsString, String> {
    let name =
        std::str::from_utf8(bytes).map_err(|_| "Remote name cannot be represented on Windows")?;
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            n.chars().count() == 1
                && n.chars()
                    .next()
                    .is_some_and(|n| matches!(n, '1'..='9' | '¹' | '²' | '³'))
        })
    });
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || reserved
        || name
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return Err("Remote entry is an unsafe Windows filename".into());
    }
    Ok(name.into())
}
fn selected_parent(root: &Dir, parts: &[&[u8]]) -> Result<Dir, String> {
    let mut selected = root.try_clone().map_err(error)?;
    for part in parts {
        selected = selected
            .open_dir_nofollow(local_component(part)?)
            .map_err(error)?;
    }
    Ok(selected)
}
#[derive(Clone)]
enum Node {
    Directory,
    File,
    Link(Vec<u8>),
}
fn link_is_directory(
    path: &[u8],
    value: &[u8],
    nodes: &std::collections::BTreeMap<Vec<u8>, Node>,
) -> Result<bool, String> {
    use std::collections::VecDeque;
    let path = super::components(path)?;
    let mut pending = path[..path.len() - 1]
        .iter()
        .map(|p| p.to_vec())
        .collect::<VecDeque<_>>();
    pending.extend(
        value
            .split(|b| *b == b'/')
            .filter(|p| !p.is_empty() && *p != b".")
            .map(|p| p.to_vec()),
    );
    let mut prefix = Vec::<Vec<u8>>::new();
    let mut followed = 0;
    while let Some(part) = pending.pop_front() {
        local_component(&part)?;
        let mut selected = prefix.clone();
        selected.push(part.clone());
        let selected = selected.join(&b'/');
        match nodes.get(&selected) {
            Some(Node::Directory) => prefix.push(part),
            Some(Node::File) if pending.is_empty() => return Ok(false),
            Some(Node::Link(value)) => {
                followed += 1; if followed > 40 { return Err("Cannot determine Windows target type for a cyclic symlink; folder staging retained".into()); }
                let mut next = value.split(|b| *b == b'/').filter(|p| !p.is_empty() && *p != b".").map(|p| p.to_vec()).collect::<VecDeque<_>>(); next.append(&mut pending); pending = next;
            }
            _ => return Err("Cannot determine Windows target type for a dangling or non-directory symlink; folder staging retained".into()),
        }
    }
    Ok(true)
}
pub(crate) async fn extract(
    mut archive: File,
    root: File,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<super::FolderModes, String> {
    tokio::task::spawn_blocking(move || {
        let root = Dir::from_std_file(root);
        archive.seek(SeekFrom::Start(0)).map_err(error)?;
        let mut magic = [0;6]; archive.read_exact(&mut magic).map_err(error)?; if &magic != b"TCAR1\0" { return Err("Unsupported folder payload".into()); }
        let number = |file: &mut File| -> Result<u32,String> { let mut bytes = [0;4]; file.read_exact(&mut bytes).map_err(error)?; Ok(u32::from_be_bytes(bytes)) };
        let root_mode = number(&mut archive)? & 0o777;
        let mut budget = Budget::default(); let mut nodes = std::collections::BTreeMap::new(); let mut modes = std::collections::HashMap::new(); let mut links = Vec::new();
        loop {
            check(&cancel)?; let mut kind = [0]; archive.read_exact(&mut kind).map_err(error)?;
            if kind[0] == 0 { if archive.read(&mut kind).map_err(error)? != 0 { return Err("Trailing folder payload".into()); } break; }
            let mode = number(&mut archive)? & 0o777; let length = number(&mut archive)? as usize;
            let mut size = [0;8]; archive.read_exact(&mut size).map_err(error)?; let size = u64::from_be_bytes(size);
            if length == 0 || length > 32768 { return Err("Folder path exceeds 32 KiB".into()); }
            let mut path = vec![0;length]; archive.read_exact(&mut path).map_err(error)?; budget.add(&path)?;
            if nodes.contains_key(&path) { return Err("Duplicate folder entry".into()); }
            let parts = super::components(&path)?;
            for part in &parts { local_component(part)?; }
            let parent = selected_parent(&root, &parts[..parts.len()-1])?; let name = local_component(parts.last().unwrap())?;
            match kind[0] {
                1 if size == 0 => { parent.create_dir(&name).map_err(error)?; modes.insert(path.clone(), mode); nodes.insert(path, Node::Directory); }
                2 => {
                    let mut options = cap_std::fs::OpenOptions::new(); options.write(true).create_new(true).follow(FollowSymlinks::No);
                    let mut file = parent.open_with(&name, &options).map_err(error)?.into_std();
                    let mut remaining = size; let mut buffer = [0;32768];
                    while remaining > 0 { check(&cancel)?; let n = remaining.min(buffer.len() as u64) as usize; archive.read_exact(&mut buffer[..n]).map_err(error)?; file.write_all(&buffer[..n]).map_err(error)?; remaining -= n as u64; }
                    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; file.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(error)?; }
                    // POSIX modes are not Windows ACLs; native ACL inheritance remains in force.
                    file.sync_all().map_err(error)?;
                    super::super::operations::close_recovery_file(file).map_err(error)?;
                    nodes.insert(path, Node::File);
                }
                3 if size <= 32768 => {
                    let mut value = vec![0;size as usize]; archive.read_exact(&mut value).map_err(error)?; link(&value)?;
                    for part in value.split(|b| *b == b'/').filter(|p| !p.is_empty() && *p != b".") { local_component(part)?; }
                    nodes.insert(path.clone(), Node::Link(value.clone())); links.push((path, value));
                }
                _ => return Err("Unsupported folder payload entry".into()),
            }
        }
        for (path, value) in links {
            check(&cancel)?; let directory = link_is_directory(&path, &value, &nodes)?;
            let parts = super::components(&path)?; let parent = selected_parent(&root, &parts[..parts.len()-1])?; let name = local_component(parts.last().unwrap())?;
            let value = std::str::from_utf8(&value).map_err(|_| "Remote link cannot be represented on Windows")?;
            #[cfg(unix)] { let _ = directory; parent.symlink(value, name).map_err(error)?; }
            #[cfg(windows)] {
                let result = if directory { parent.symlink_dir(value, name) } else { parent.symlink_file(value, name) };
                result.map_err(|e| format!("Windows could not preserve a symlink; symlink permission or Developer Mode may be required: {e}"))?;
            }
        }
        Ok(super::FolderModes { root: root_mode, directories: modes })
    }).await.map_err(error)?
}
#[cfg(windows)]
pub(crate) async fn finalize(_root: File, _modes: super::FolderModes) -> Result<(), String> {
    Ok(())
}
