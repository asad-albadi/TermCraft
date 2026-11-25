use super::{read_bounded, read_regular};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

const FILE_LIMIT: u64 = 8 * 1024 * 1024;
const FOLDER_LIMIT: u64 = 128 * 1024 * 1024;
const ENTRY_LIMIT: usize = 10_000;
const DEPTH_LIMIT: usize = 32;
const TEXT_LIMIT: usize = 128 * 1024;
const LINE_LIMIT: usize = 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
}

#[derive(Debug)]
pub struct DiffResult {
    pub equal: bool,
    /// Line edits for bounded UTF-8 text. Binary and large text comparisons are byte-only.
    pub changes: Option<Vec<Change>>,
}

pub struct FileDiff;

impl FileDiff {
    pub fn compare(left: &Path, right: &Path) -> Result<DiffResult, String> {
        let left = read_regular(left, FILE_LIMIT)?;
        let right = read_regular(right, FILE_LIMIT)?;
        let equal = left == right;
        Ok(DiffResult {
            equal,
            changes: if equal {
                Some(Vec::new())
            } else {
                line_changes(&left, &right)
            },
        })
    }
}

fn line_changes(left: &[u8], right: &[u8]) -> Option<Vec<Change>> {
    if left.len() + right.len() > TEXT_LIMIT || left.contains(&0) || right.contains(&0) {
        return None;
    }
    let left = std::str::from_utf8(left)
        .ok()?
        .split_inclusive('\n')
        .take(LINE_LIMIT + 1)
        .collect::<Vec<_>>();
    let right = std::str::from_utf8(right)
        .ok()?
        .split_inclusive('\n')
        .take(LINE_LIMIT + 1)
        .collect::<Vec<_>>();
    if left.len() > LINE_LIMIT || right.len() > LINE_LIMIT {
        return None;
    }

    // Keeping newline bytes distinguishes a missing final newline and CRLF from LF.
    let columns = right.len() + 1;
    let mut common = vec![0u16; (left.len() + 1) * columns];
    for (i, a) in left.iter().enumerate().rev() {
        for (j, b) in right.iter().enumerate().rev() {
            let index = i * columns + j;
            common[index] = if a == b {
                common[index + columns + 1] + 1
            } else {
                common[index + columns].max(common[index + 1])
            };
        }
    }

    let (mut i, mut j) = (0, 0);
    let mut changes = Vec::new();
    while i < left.len() || j < right.len() {
        if i < left.len() && j < right.len() && left[i] == right[j] {
            i += 1;
            j += 1;
        } else if i < left.len()
            && (j == right.len() || common[(i + 1) * columns + j] >= common[i * columns + j + 1])
        {
            changes.push(Change::Removed(left[i].to_owned()));
            i += 1;
        } else {
            changes.push(Change::Added(right[j].to_owned()));
            j += 1;
        }
    }
    Some(changes)
}

#[derive(Debug, Default)]
pub struct FolderDiffResult {
    pub added: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
}

pub struct FolderDiff;

#[derive(Clone, Copy)]
enum Entry {
    Directory,
    File,
}

impl FolderDiff {
    pub fn compare(left: &Path, right: &Path) -> Result<FolderDiffResult, String> {
        Self::compare_with_cancel(left, right, &|| false)
    }

    pub fn compare_with_cancel(
        left: &Path,
        right: &Path,
        cancelled: &impl Fn() -> bool,
    ) -> Result<FolderDiffResult, String> {
        check_cancel(cancelled)?;
        let left = FolderRoot::open(left)?;
        let right = FolderRoot::open(right)?;
        let left_entries = left.list(cancelled)?;
        let right_entries = right.list(cancelled)?;
        compare_entries(&left, &left_entries, &right, &right_entries, cancelled)
    }
}

fn check_cancel(cancelled: &impl Fn() -> bool) -> Result<(), String> {
    if cancelled() {
        Err("Folder comparison cancelled".into())
    } else {
        Ok(())
    }
}

struct FolderRoot {
    path: PathBuf,
    dir: Dir,
}

impl FolderRoot {
    fn open(path: &Path) -> Result<Self, String> {
        let dir = if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            let dir = Dir::open_ambient_dir(parent, ambient_authority())
                .map_err(|error| format!("{}: {error}", parent.display()))?;
            open_directory(&dir, name, path)?
        } else {
            Dir::open_ambient_dir(path, ambient_authority())
                .map_err(|error| format!("{}: {error}", path.display()))?
        };
        Ok(Self {
            path: path.to_owned(),
            dir,
        })
    }

    fn list(&self, cancelled: &impl Fn() -> bool) -> Result<BTreeMap<PathBuf, Entry>, String> {
        let mut entries = BTreeMap::new();
        self.walk(&self.dir, Path::new(""), 0, &mut entries, cancelled)?;
        Ok(entries)
    }

    fn walk(
        &self,
        dir: &Dir,
        relative: &Path,
        depth: usize,
        entries: &mut BTreeMap<PathBuf, Entry>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), String> {
        check_cancel(cancelled)?;
        if depth > DEPTH_LIMIT {
            return Err("Folder comparison exceeded its directory depth limit".into());
        }
        let display = self.path.join(relative);
        let children = dir
            .entries()
            .map_err(|error| format!("{}: {error}", display.display()))?;
        for child in children {
            check_cancel(cancelled)?;
            if entries.len() >= ENTRY_LIMIT {
                return Err("Folder comparison exceeded its entry limit".into());
            }
            let child = child.map_err(|error| format!("{}: {error}", display.display()))?;
            let component = child.file_name();
            let name = relative.join(&component);
            let display = self.path.join(&name);
            let metadata = dir
                .symlink_metadata(&component)
                .map_err(|error| format!("{}: {error}", display.display()))?;
            let entry = kind(&metadata, &display)?;
            entries.insert(name.clone(), entry);
            if matches!(entry, Entry::Directory) {
                let opened = open_directory(dir, &component, &display)?;
                self.walk(&opened, &name, depth + 1, entries, cancelled)?;
            }
        }
        Ok(())
    }

    fn read_file(&self, relative: &Path, limit: u64) -> Result<Vec<u8>, String> {
        let mut dir = self
            .dir
            .try_clone()
            .map_err(|error| format!("{}: {error}", self.path.display()))?;
        let parent = relative.parent().expect("entries are relative paths");
        let mut display = self.path.clone();
        for component in parent {
            display.push(component);
            dir = open_directory(&dir, component, &display)?;
        }
        let name = relative.file_name().expect("entries have a name");
        display.push(name);
        let metadata = dir
            .symlink_metadata(name)
            .map_err(|error| format!("{}: {error}", display.display()))?;
        if !matches!(kind(&metadata, &display)?, Entry::File) {
            return Err(format!("{}: expected a regular file", display.display()));
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = dir
            .open_with(name, &options)
            .map_err(|error| format!("{}: {error}", display.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("{}: {error}", display.display()))?;
        if !matches!(kind(&metadata, &display)?, Entry::File) {
            return Err(format!("{}: expected a regular file", display.display()));
        }
        read_bounded(file, metadata.len(), &display, limit)
    }
}

fn compare_entries(
    left: &FolderRoot,
    left_entries: &BTreeMap<PathBuf, Entry>,
    right: &FolderRoot,
    right_entries: &BTreeMap<PathBuf, Entry>,
    cancelled: &impl Fn() -> bool,
) -> Result<FolderDiffResult, String> {
    let names = left_entries
        .keys()
        .chain(right_entries.keys())
        .collect::<BTreeSet<_>>();
    let mut result = FolderDiffResult::default();
    let mut remaining = FOLDER_LIMIT;
    for name in names {
        check_cancel(cancelled)?;
        match (left_entries.get(name), right_entries.get(name)) {
            (None, Some(_)) => result.added.push(name.clone()),
            (Some(_), None) => result.removed.push(name.clone()),
            (Some(Entry::File), Some(Entry::File)) => {
                let a = left.read_file(name, FILE_LIMIT.min(remaining))?;
                remaining -= a.len() as u64;
                check_cancel(cancelled)?;
                let b = right.read_file(name, FILE_LIMIT.min(remaining))?;
                remaining -= b.len() as u64;
                if a != b {
                    result.changed.push(name.clone());
                }
            }
            (Some(Entry::Directory), Some(Entry::Directory)) => {}
            (Some(_), Some(_)) => result.changed.push(name.clone()),
            (None, None) => unreachable!(),
        }
    }
    check_cancel(cancelled)?;
    Ok(result)
}

fn open_directory(dir: &Dir, name: &OsStr, display: &Path) -> Result<Dir, String> {
    let metadata = dir
        .symlink_metadata(name)
        .map_err(|error| format!("{}: {error}", display.display()))?;
    if !matches!(kind(&metadata, display)?, Entry::Directory) {
        return Err(format!("{}: expected a directory", display.display()));
    }
    let opened = dir
        .open_dir_nofollow(name)
        .map_err(|error| format!("{}: {error}", display.display()))?;
    let metadata = opened
        .dir_metadata()
        .map_err(|error| format!("{}: {error}", display.display()))?;
    if !matches!(kind(&metadata, display)?, Entry::Directory) {
        return Err(format!("{}: expected a directory", display.display()));
    }
    Ok(opened)
}

fn kind(metadata: &Metadata, path: &Path) -> Result<Entry, String> {
    #[cfg(windows)]
    use cap_std::fs::MetadataExt;
    #[cfg(windows)]
    let redirected = metadata.file_attributes() & 0x400 != 0;
    #[cfg(not(windows))]
    let redirected = metadata.is_symlink();
    if redirected {
        return Err(format!(
            "{}: symbolic link or redirected entry is unsupported",
            path.display()
        ));
    }
    if metadata.is_dir() {
        Ok(Entry::Directory)
    } else if metadata.is_file() {
        Ok(Entry::File)
    } else {
        Err(format!("{}: unsupported filesystem entry", path.display()))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn folder_diff_stays_on_open_root_after_path_replacement() {
        let fixture = tempfile::tempdir().unwrap();
        let left = fixture.path().join("left");
        let right = fixture.path().join("right");
        let outside = fixture.path().join("outside");
        for path in [&left, &right, &outside] {
            fs::create_dir(path).unwrap();
        }
        fs::write(left.join("item"), "same").unwrap();
        fs::write(right.join("item"), "same").unwrap();
        fs::write(outside.join("item"), "outside").unwrap();

        let left_root = FolderRoot::open(&left).unwrap();
        let right_root = FolderRoot::open(&right).unwrap();
        let left_entries = left_root.list(&|| false).unwrap();
        let right_entries = right_root.list(&|| false).unwrap();
        fs::rename(&left, fixture.path().join("moved")).unwrap();
        symlink(outside, left).unwrap();

        let result = compare_entries(
            &left_root,
            &left_entries,
            &right_root,
            &right_entries,
            &|| false,
        )
        .unwrap();
        assert!(result.changed.is_empty());
    }

    #[test]
    fn folder_diff_refuses_intermediate_symlink_replacement_after_listing() {
        let fixture = tempfile::tempdir().unwrap();
        let left = fixture.path().join("left");
        let right = fixture.path().join("right");
        let outside = fixture.path().join("outside");
        for path in [left.join("nested"), right.join("nested"), outside.clone()] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(left.join("nested/item"), "same").unwrap();
        fs::write(right.join("nested/item"), "same").unwrap();
        fs::write(outside.join("item"), "outside").unwrap();

        let left_root = FolderRoot::open(&left).unwrap();
        let right_root = FolderRoot::open(&right).unwrap();
        let left_entries = left_root.list(&|| false).unwrap();
        let right_entries = right_root.list(&|| false).unwrap();
        fs::rename(left.join("nested"), fixture.path().join("moved")).unwrap();
        symlink(outside, left.join("nested")).unwrap();

        assert!(
            compare_entries(
                &left_root,
                &left_entries,
                &right_root,
                &right_entries,
                &|| false
            )
            .is_err()
        );
    }
}
