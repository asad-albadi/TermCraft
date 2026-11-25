//! Application-owned NFSv3/TCP and MOUNTv3 export. No system mapper or daemon.
use super::files::Share;
use async_trait::async_trait;
use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, File, Metadata, OpenOptions};
use nfsembed::vfs::*;
use nfsembed::{AuthPolicy, NfsServer, ServerLimits};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, Read, Seek, SeekFrom, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
const MAX_OBJECTS: usize = 4096;
const MAX_CREATED: usize = 256;
const MAX_TRANSFER: u32 = 64 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct Listener {
    socket: TcpListener,
    export: Export,
}
impl Listener {
    pub(super) fn bind(bind: SocketAddr, root: &Path, writable: bool) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("Choose an absolute shared folder path.".into());
        }
        let export =
            Export::open(root, writable).map_err(|e| format!("Could not open NFS export: {e}"))?;
        let socket =
            TcpListener::bind(bind).map_err(|e| format!("Could not bind NFS listener: {e}"))?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self { socket, export })
    }
    pub(super) fn address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
    pub(super) fn serve(self, cancelled: impl Fn() -> bool) -> io::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::from_std(self.socket)?;
            let server = NfsServer::builder(self.export).export_name("/share").auth_policy(AuthPolicy::AuthSysOrAnonymous).limits(limits()).build().map_err(io::Error::other)?;
            let handle = server.start(listener).await.map_err(io::Error::other)?;
            let result = tokio::select! {
                result = handle.wait() => result,
                _ = async { while !cancelled() { tokio::time::sleep(Duration::from_millis(50)).await; } } => {
                    handle.shutdown().await.map_err(io::Error::other)?;
                    // shutdown only signals: wait drains connections and execution JoinSets.
                    handle.wait().await
                }
            };
            result.map_err(io::Error::other)
        })
    }
}
fn limits() -> ServerLimits {
    ServerLimits {
        max_connections: 8,
        max_requests_per_connection: 2,
        max_inflight_requests: 8,
        max_rpc_record_size: 128 * 1024,
        max_rpc_fragment_size: 128 * 1024,
        max_fragments_per_record: 4,
        max_buffered_request_bytes: 1024 * 1024,
        max_buffered_reply_bytes: 1024 * 1024,
        max_read_size: MAX_TRANSFER,
        max_write_size: MAX_TRANSFER,
        max_readdir_response_size: MAX_TRANSFER,
        request_timeout: Duration::from_secs(5),
        idle_connection_timeout: Duration::from_secs(10),
        replay_cache_capacity: 128,
        replay_cache_max_bytes: 1024 * 1024,
        replay_cache_ttl: Duration::from_secs(30),
        graceful_shutdown_timeout: Duration::from_secs(1),
        max_mounts: 16,
    }
}
enum Handle {
    Directory(Dir),
    File(File),
}
impl Handle {
    fn metadata(&self) -> io::Result<Metadata> {
        match self {
            Self::Directory(d) => d.dir_metadata(),
            Self::File(f) => f.metadata(),
        }
    }
}
struct Object {
    path: PathBuf,
    handle: Handle,
    exclusive: Option<[u8; 8]>,
}
struct State {
    objects: BTreeMap<u64, Object>,
    next: u64,
    created: usize,
    written: u64,
    expanded: u64,
    epoch: u64,
}
struct Export {
    share: Share,
    state: Mutex<State>,
    generation: u64,
    writable: bool,
}
impl Export {
    fn open(root: &Path, writable: bool) -> io::Result<Self> {
        let share = Share::open(root, writable)?;
        let dir = share.directory(Path::new(""))?;
        let mut random = [0u8; 8];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("Could not assign export generation"))?;
        let generation = u64::from_ne_bytes(random);
        Ok(Self {
            share,
            state: Mutex::new(State {
                objects: BTreeMap::from([(
                    1,
                    Object {
                        path: PathBuf::new(),
                        handle: Handle::Directory(dir),
                        exclusive: None,
                    },
                )]),
                next: 2,
                created: 0,
                written: 0,
                expanded: 0,
                epoch: 1,
            }),
            generation,
            writable,
        })
    }
    fn validate<'a>(&self, state: &'a State, key: ObjectKey) -> Result<&'a Object, NfsError> {
        if key.generation != self.generation {
            return Err(NfsError::Stale);
        }
        let object = state.objects.get(&key.file_id).ok_or(NfsError::Stale)?;
        if !object.path.as_os_str().is_empty() {
            let current = self
                .open_handle(&object.path, false)
                .map_err(|_| NfsError::Stale)?;
            let a = object.handle.metadata().map_err(err)?;
            let b = current.metadata().map_err(err)?;
            if a.dev() != b.dev() || a.ino() != b.ino() || a.is_dir() != b.is_dir() {
                return Err(NfsError::Stale);
            }
        }
        Ok(object)
    }
    fn child(&self, state: &State, parent: ObjectKey, name: &NfsName) -> Result<PathBuf, NfsError> {
        let object = self.validate(state, parent)?;
        if !matches!(object.handle, Handle::Directory(_)) {
            return Err(NfsError::NotDirectory);
        }
        if object.path.components().count() >= 64 {
            return Err(NfsError::NameTooLong);
        }
        Ok(object.path.join(os_name(name)?))
    }
    fn open_handle(&self, path: &Path, write: bool) -> io::Result<Handle> {
        if self.share.kind(path)?.is_dir() {
            return self.share.directory(path).map(Handle::Directory);
        }
        if !write || !self.writable {
            return self.share.read(path).map(Handle::File);
        }
        let dir = self
            .share
            .directory(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NONBLOCK);
        }
        let file = dir.open_with(
            path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
            &options,
        )?;
        check_mutable_file(&file)?;
        Ok(Handle::File(file))
    }
    fn intern(
        &self,
        state: &mut State,
        path: PathBuf,
        handle: Handle,
        _created: bool,
    ) -> Result<CreatedObject, NfsError> {
        let meta = handle.metadata().map_err(err)?;
        for (&id, old) in &state.objects {
            if old.path == path {
                let previous = old.handle.metadata().map_err(err)?;
                if previous.dev() == meta.dev() && previous.ino() == meta.ino() {
                    return Ok(CreatedObject {
                        object: ObjectKey {
                            file_id: id,
                            generation: self.generation,
                        },
                        attributes: Some(attributes(id, &meta, self.writable)),
                    });
                }
            }
        }
        if state.objects.len() >= MAX_OBJECTS {
            return Err(NfsError::Jukebox);
        }
        let id = state.next;
        state.next += 1;
        state.objects.insert(
            id,
            Object {
                path,
                handle,
                exclusive: None,
            },
        );
        Ok(CreatedObject {
            object: ObjectKey {
                file_id: id,
                generation: self.generation,
            },
            attributes: Some(attributes(id, &meta, self.writable)),
        })
    }
    fn writeable(&self) -> Result<(), NfsError> {
        if self.writable {
            Ok(())
        } else {
            Err(NfsError::ReadOnly)
        }
    }
    fn mutation<T>(&self, value: T) -> MutationResult<T> {
        MutationResult {
            value,
            before: None,
            after: None,
        }
    }
    fn check_attributes(&self, attrs: &SetAttributes) -> Result<(), NfsError> {
        if attrs.uid.is_some()
            || attrs.gid.is_some()
            || attrs.access_time.is_some()
            || attrs.modify_time.is_some()
            || attrs.size.is_some_and(|s| s > MAX_BYTES)
        {
            Err(NfsError::NotSupported)
        } else {
            Ok(())
        }
    }
}
fn os_name(name: &NfsName) -> Result<OsString, NfsError> {
    let bytes = name.as_bytes();
    if bytes == b"."
        || bytes == b".."
        || bytes
            .iter()
            .any(|b| *b == b'\\' || *b == b':' || b.is_ascii_control())
    {
        return Err(NfsError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(windows)]
    {
        let name = std::str::from_utf8(bytes).map_err(|_| NfsError::Invalid)?;
        if !super::files::usable_name(name) {
            return Err(NfsError::Invalid);
        }
        Ok(OsString::from(name))
    }
}
fn wire_name(name: OsString) -> Result<NfsName, NfsError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        NfsName::new(name.into_vec())
    }
    #[cfg(windows)]
    {
        NfsName::new(
            name.into_string()
                .map_err(|_| NfsError::Invalid)?
                .into_bytes(),
        )
    }
}
// Multi-linked regular files may have aliases outside the selected root.
// Check the held descriptor; a pathname check would race replacement. This is
// conservative observed-link policy, not a kernel-atomic link-count transaction.
fn check_mutable_file(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "NFS writes require a regular file with exactly one hard link",
        ));
    }
    Ok(())
}
fn err(e: io::Error) -> NfsError {
    match e.kind() {
        io::ErrorKind::NotFound => NfsError::NotFound,
        io::ErrorKind::AlreadyExists => NfsError::Exists,
        io::ErrorKind::PermissionDenied => NfsError::Access,
        io::ErrorKind::InvalidInput => NfsError::Invalid,
        io::ErrorKind::DirectoryNotEmpty => NfsError::NotEmpty,
        _ => NfsError::Io,
    }
}
fn attributes(id: u64, m: &Metadata, writable: bool) -> FileAttributes {
    let time = |t: io::Result<cap_std::time::SystemTime>| {
        t.ok()
            .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| NfsTime {
                seconds: d.as_secs(),
                nanoseconds: d.subsec_nanos(),
            })
            .unwrap_or_default()
    };
    FileAttributes {
        file_type: if m.is_dir() {
            FileType::Directory
        } else {
            FileType::Regular
        },
        mode: if m.is_dir() {
            if writable { 0o700 } else { 0o500 }
        } else if writable {
            0o600
        } else {
            0o400
        },
        links: 1,
        uid: 0,
        gid: 0,
        size: m.len(),
        used: m.len(),
        device: None,
        fs_id: 1,
        file_id: id,
        access_time: time(m.accessed()),
        modify_time: time(m.modified()),
        change_time: time(m.modified()),
    }
}
#[async_trait]
impl VirtualFileSystem for Export {
    fn capabilities(&self) -> VfsCapabilities {
        VfsCapabilities {
            read_only: !self.writable,
            ..VfsCapabilities::READ_ONLY
        }
    }
    fn root(&self) -> ObjectKey {
        ObjectKey {
            file_id: 1,
            generation: self.generation,
        }
    }
    async fn getattr(
        &self,
        _: &RequestContext,
        key: ObjectKey,
    ) -> Result<FileAttributes, NfsError> {
        let state = self.state.lock().unwrap();
        let o = self.validate(&state, key)?;
        Ok(attributes(
            key.file_id,
            &o.handle.metadata().map_err(err)?,
            self.writable,
        ))
    }
    async fn lookup(
        &self,
        _: &RequestContext,
        parent: ObjectKey,
        name: &NfsName,
    ) -> Result<CreatedObject, NfsError> {
        let mut s = self.state.lock().unwrap();
        let path = self.child(&s, parent, name)?;
        let handle = self.open_handle(&path, self.writable).map_err(err)?;
        self.intern(&mut s, path, handle, false)
    }
    async fn access(
        &self,
        _: &RequestContext,
        key: ObjectKey,
        requested: u32,
    ) -> Result<u32, NfsError> {
        let s = self.state.lock().unwrap();
        let object = self.validate(&s, key)?;
        let mutable = self.writable
            && match &object.handle {
                Handle::File(file) => check_mutable_file(file).is_ok(),
                Handle::Directory(_) => true,
            };
        let allowed = if mutable { 0x3f } else { 0x23 };
        Ok(requested & allowed)
    }
    async fn setattr(
        &self,
        _: &RequestContext,
        key: ObjectKey,
        attrs: SetAttributes,
        guard: Option<NfsTime>,
    ) -> Result<MutationResult<()>, NfsError> {
        self.writeable()?;
        self.check_attributes(&attrs)?;
        if guard.is_some() || attrs.mode.is_some() {
            return Err(NfsError::NotSupported);
        }
        let mut s = self.state.lock().unwrap();
        let object = self.validate(&s, key)?;
        let Handle::File(file) = &object.handle else {
            return Err(NfsError::IsDirectory);
        };
        let file = file.try_clone().map_err(err)?;
        if let Some(size) = attrs.size {
            if s.expanded.saturating_add(size) > MAX_BYTES {
                return Err(NfsError::Quota);
            }
            s.expanded += size;
            check_mutable_file(&file).map_err(err)?;
            file.set_len(size).map_err(err)?;
            file.sync_all().map_err(err)?;
        }
        let after = attributes(key.file_id, &file.metadata().map_err(err)?, self.writable);
        Ok(MutationResult {
            value: (),
            before: None,
            after: Some(after),
        })
    }
    async fn read(
        &self,
        _: &RequestContext,
        key: ObjectKey,
        offset: u64,
        count: u32,
    ) -> Result<ReadResult, NfsError> {
        let s = self.state.lock().unwrap();
        let o = self.validate(&s, key)?;
        let Handle::File(file) = &o.handle else {
            return Err(NfsError::IsDirectory);
        };
        let meta = file.metadata().map_err(err)?;
        let mut f = file.try_clone().map_err(err)?;
        f.seek(SeekFrom::Start(offset)).map_err(err)?;
        let mut data = vec![
            0;
            (count.min(MAX_TRANSFER) as u64).min(meta.len().saturating_sub(offset))
                as usize
        ];
        f.read_exact(&mut data).map_err(err)?;
        Ok(ReadResult {
            eof: offset.saturating_add(data.len() as u64) >= meta.len(),
            data,
            attributes: Some(attributes(key.file_id, &meta, self.writable)),
        })
    }
    async fn write(
        &self,
        _: &RequestContext,
        key: ObjectKey,
        offset: u64,
        data: &[u8],
        _: WriteStability,
    ) -> Result<MutationResult<WriteResult>, NfsError> {
        self.writeable()?;
        let mut s = self.state.lock().unwrap();
        let o = self.validate(&s, key)?;
        if data.len() > MAX_TRANSFER as usize
            || offset
                .checked_add(data.len() as u64)
                .is_none_or(|n| n > MAX_BYTES)
            || s.written.saturating_add(data.len() as u64) > MAX_BYTES
        {
            return Err(NfsError::FileTooLarge);
        }
        let Handle::File(file) = &o.handle else {
            return Err(NfsError::IsDirectory);
        };
        let growth = offset
            .saturating_add(data.len() as u64)
            .saturating_sub(file.metadata().map_err(err)?.len());
        if s.expanded.saturating_add(growth) > MAX_BYTES {
            return Err(NfsError::Quota);
        }
        let mut f = file.try_clone().map_err(err)?;
        s.written += data.len() as u64;
        s.expanded += growth;
        f.seek(SeekFrom::Start(offset)).map_err(err)?;
        check_mutable_file(&f).map_err(err)?;
        f.write_all(data).map_err(err)?;
        f.sync_all().map_err(err)?;
        Ok(self.mutation(WriteResult {
            count: data.len() as u32,
            committed: WriteStability::FileSync,
        }))
    }
    async fn create(
        &self,
        _: &RequestContext,
        parent: ObjectKey,
        name: &NfsName,
        attrs: SetAttributes,
        mode: CreateMode,
    ) -> Result<MutationResult<CreatedObject>, NfsError> {
        self.writeable()?;
        self.check_attributes(&attrs)?;
        let mut s = self.state.lock().unwrap();
        let path = self.child(&s, parent, name)?;
        match self.open_handle(&path, self.writable) {
            Ok(handle) => {
                let mut object = self.intern(&mut s, path.clone(), handle, false)?;
                match mode {
                    CreateMode::Guarded => return Err(NfsError::Exists),
                    CreateMode::Exclusive { verifier } => {
                        if s.objects.get(&object.object.file_id).unwrap().exclusive
                            != Some(verifier)
                        {
                            return Err(NfsError::Exists);
                        }
                    }
                    CreateMode::Unchecked => {
                        if object.attributes.as_ref().unwrap().file_type != FileType::Regular {
                            return Err(NfsError::IsDirectory);
                        }
                        if let Some(size) = attrs.size {
                            if s.expanded.saturating_add(size) > MAX_BYTES {
                                return Err(NfsError::Quota);
                            }
                            let Handle::File(file) =
                                &s.objects.get(&object.object.file_id).unwrap().handle
                            else {
                                return Err(NfsError::IsDirectory);
                            };
                            let file = file.try_clone().map_err(err)?;
                            s.expanded += size;
                            check_mutable_file(&file).map_err(err)?;
                            file.set_len(size).map_err(err)?;
                            file.sync_all().map_err(err)?;
                            object.attributes = Some(attributes(
                                object.object.file_id,
                                &file.metadata().map_err(err)?,
                                self.writable,
                            ));
                        }
                    }
                }
                return Ok(self.mutation(object));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(err(error)),
        }
        if s.created >= MAX_CREATED
            || s.objects.len() >= MAX_OBJECTS
            || s.expanded.saturating_add(attrs.size.unwrap_or(0)) > MAX_BYTES
        {
            return Err(NfsError::Quota);
        }
        // Reserve before mutating: partial I/O and sync failures still consume the budgets.
        s.created += 1;
        s.expanded += attrs.size.unwrap_or(0);
        // Atomic creation. Existing UNCHECKED files are handled through the pinned descriptor above.
        let parent_dir = self.share.directory(path.parent().unwrap()).map_err(err)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = parent_dir
            .open_with(path.file_name().unwrap(), &options)
            .map_err(err)?;
        if let Some(size) = attrs.size {
            check_mutable_file(&file).map_err(err)?;
            file.set_len(size).map_err(err)?;
        }
        file.sync_all().map_err(err)?;
        s.epoch += 1;
        let object = self.intern(&mut s, path, Handle::File(file), true)?;
        if let CreateMode::Exclusive { verifier } = mode {
            s.objects.get_mut(&object.object.file_id).unwrap().exclusive = Some(verifier);
        }
        Ok(self.mutation(object))
    }
    async fn mkdir(
        &self,
        _: &RequestContext,
        parent: ObjectKey,
        name: &NfsName,
        attrs: SetAttributes,
    ) -> Result<MutationResult<CreatedObject>, NfsError> {
        self.writeable()?;
        self.check_attributes(&attrs)?;
        let mut s = self.state.lock().unwrap();
        if s.created >= MAX_CREATED || s.objects.len() >= MAX_OBJECTS {
            return Err(NfsError::Quota);
        }
        let path = self.child(&s, parent, name)?;
        s.created += 1;
        self.share.create_directory(&path).map_err(err)?;
        let handle = self.open_handle(&path, false).map_err(err)?;
        s.epoch += 1;
        let object = self.intern(&mut s, path, handle, true)?;
        Ok(self.mutation(object))
    }
    async fn remove(
        &self,
        _: &RequestContext,
        parent: ObjectKey,
        name: &NfsName,
    ) -> Result<MutationResult<()>, NfsError> {
        self.remove_object(parent, name, false)
    }
    async fn rmdir(
        &self,
        _: &RequestContext,
        parent: ObjectKey,
        name: &NfsName,
    ) -> Result<MutationResult<()>, NfsError> {
        self.remove_object(parent, name, true)
    }
    async fn rename(
        &self,
        _: &RequestContext,
        from: ObjectKey,
        from_name: &NfsName,
        to: ObjectKey,
        to_name: &NfsName,
    ) -> Result<(MutationResult<()>, MutationResult<()>), NfsError> {
        self.writeable()?;
        let mut s = self.state.lock().unwrap();
        let source = self.child(&s, from, from_name)?;
        let dest = self.child(&s, to, to_name)?;
        // Full write mode authorizes normal destination replacement. No target CAS is promised.
        // Keep a pinned source and opened parent directories across the rename.
        let source_handle = self.open_handle(&source, false).map_err(err)?;
        let source_dir = self
            .share
            .directory(source.parent().unwrap())
            .map_err(err)?;
        let destination_dir = self.share.directory(dest.parent().unwrap()).map_err(err)?;
        if source == dest {
            return Ok((self.mutation(()), self.mutation(())));
        }
        if let Ok(kind) = destination_dir.symlink_metadata(dest.file_name().unwrap()) {
            if !kind.is_file() && !kind.is_dir() {
                return Err(NfsError::Access);
            }
        }
        source_dir
            .rename(
                source.file_name().unwrap(),
                &destination_dir,
                dest.file_name().unwrap(),
            )
            .map_err(err)?;
        drop(source_handle);
        s.objects.retain(|_, o| !o.path.starts_with(&dest));
        for o in s.objects.values_mut() {
            if let Ok(suffix) = o.path.strip_prefix(&source) {
                o.path = dest.join(suffix)
            }
        }
        s.epoch += 1;
        Ok((self.mutation(()), self.mutation(())))
    }
    async fn readdir(
        &self,
        _: &RequestContext,
        key: ObjectKey,
        cookie: u64,
        verifier: [u8; 8],
        hint: usize,
    ) -> Result<ReadDirectoryPage, NfsError> {
        let mut s = self.state.lock().unwrap();
        let object = self.validate(&s, key)?;
        let Handle::Directory(dir) = &object.handle else {
            return Err(NfsError::NotDirectory);
        };
        let path = object.path.clone();
        let mut names = Vec::new();
        for (index, entry) in dir
            .entries()
            .map_err(err)?
            .take(MAX_OBJECTS + 1)
            .enumerate()
        {
            let entry = entry.map_err(err)?;
            if index >= MAX_OBJECTS {
                return Err(NfsError::Jukebox);
            }
            let name = wire_name(entry.file_name())?;
            if os_name(&name).is_ok() {
                names.push(name)
            }
        }
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        use std::hash::{Hash, Hasher};
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        s.epoch.hash(&mut fingerprint);
        names.hash(&mut fingerprint);
        let current_verifier = fingerprint.finish().to_be_bytes();
        if cookie != 0 && verifier != current_verifier {
            return Err(NfsError::BadCookie);
        }
        let start = usize::try_from(cookie).map_err(|_| NfsError::BadCookie)?;
        if start > names.len() {
            return Err(NfsError::BadCookie);
        }
        let mut entries = Vec::new();
        let total = names.len();
        let mut consumed = start;
        for (index, name) in names.into_iter().enumerate().skip(start) {
            consumed = index + 1;
            let child = path.join(os_name(&name)?);
            let handle = match self.open_handle(&child, self.writable) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let object = self.intern(&mut s, child, handle, false)?;
            entries.push(DirectoryEntry {
                object: object.object,
                file_id: object.object.file_id,
                name,
                cookie: (index + 1) as u64,
                attributes: object.attributes,
            });
            if entries.len() >= hint.clamp(1, 128) {
                break;
            }
        }
        Ok(ReadDirectoryPage {
            verifier: current_verifier,
            entries,
            eof: consumed == total,
        })
    }
    async fn fsinfo(&self, c: &RequestContext, key: ObjectKey) -> Result<FsInfo, NfsError> {
        self.getattr(c, key).await?;
        Ok(FsInfo {
            max_read: MAX_TRANSFER,
            preferred_read: MAX_TRANSFER,
            read_multiple: 1,
            max_write: MAX_TRANSFER,
            preferred_write: MAX_TRANSFER,
            write_multiple: 1,
            preferred_readdir: 8192,
            max_file_size: MAX_BYTES,
            time_granularity: NfsTime {
                seconds: 1,
                nanoseconds: 0,
            },
        })
    }
    async fn fsstat(&self, c: &RequestContext, key: ObjectKey) -> Result<FsStat, NfsError> {
        self.getattr(c, key).await?;
        let s = self.state.lock().unwrap();
        Ok(FsStat {
            total_bytes: MAX_BYTES,
            free_bytes: MAX_BYTES - s.written.max(s.expanded),
            available_bytes: MAX_BYTES - s.written.max(s.expanded),
            total_files: MAX_CREATED as u64,
            free_files: (MAX_CREATED - s.created) as u64,
            available_files: (MAX_CREATED - s.created) as u64,
            invariant_seconds: 0,
        })
    }
    async fn pathconf(&self, c: &RequestContext, key: ObjectKey) -> Result<PathConf, NfsError> {
        self.getattr(c, key).await?;
        Ok(PathConf {
            max_links: 1,
            max_name_length: 255,
            no_truncation: true,
            chown_restricted: true,
            case_insensitive: cfg!(windows),
            case_preserving: true,
        })
    }
    async fn commit(
        &self,
        c: &RequestContext,
        key: ObjectKey,
        _: u64,
        _: u32,
    ) -> Result<MutationResult<()>, NfsError> {
        self.writeable()?;
        self.getattr(c, key).await?;
        Ok(self.mutation(()))
    }
}
impl Export {
    fn remove_object(
        &self,
        parent: ObjectKey,
        name: &NfsName,
        directory: bool,
    ) -> Result<MutationResult<()>, NfsError> {
        self.writeable()?;
        let mut s = self.state.lock().unwrap();
        let path = self.child(&s, parent, name)?;
        self.share.remove(&path, directory).map_err(err)?;
        s.objects.retain(|_, o| !o.path.starts_with(&path));
        s.epoch += 1;
        Ok(self.mutation(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn readonly_and_created_write_boundary() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("existing"), b"original").unwrap();
        let vfs = Export::open(root.path(), false).unwrap();
        let context = context();
        let name = NfsName::new(b"existing".to_vec()).unwrap();
        let object = vfs
            .lookup(&context, vfs.root(), &name)
            .await
            .unwrap()
            .object;
        assert_eq!(
            vfs.read(&context, object, 0, 20).await.unwrap().data,
            b"original"
        );
        assert_eq!(
            vfs.write(&context, object, 0, b"bad", WriteStability::FileSync)
                .await,
            Err(NfsError::ReadOnly)
        );
        assert_eq!(
            vfs.remove(&context, vfs.root(), &name).await,
            Err(NfsError::ReadOnly)
        );
        assert_eq!(
            vfs.mkdir(&context, vfs.root(), &name, SetAttributes::default())
                .await,
            Err(NfsError::ReadOnly)
        );
        assert_eq!(
            vfs.setattr(
                &context,
                object,
                SetAttributes {
                    size: Some(0),
                    ..Default::default()
                },
                None
            )
            .await,
            Err(NfsError::ReadOnly)
        );
        assert_eq!(
            vfs.create(
                &context,
                vfs.root(),
                &name,
                SetAttributes::default(),
                CreateMode::Unchecked
            )
            .await,
            Err(NfsError::ReadOnly)
        );
        let vfs = Export::open(root.path(), true).unwrap();
        let existing = vfs
            .lookup(&context, vfs.root(), &name)
            .await
            .unwrap()
            .object;
        vfs.write(&context, existing, 0, b"updated", WriteStability::FileSync)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(root.path().join("existing")).unwrap(),
            b"updatedl"
        );
        let new = vfs
            .create(
                &context,
                vfs.root(),
                &NfsName::new(b"new".to_vec()).unwrap(),
                SetAttributes::default(),
                CreateMode::Guarded,
            )
            .await
            .unwrap()
            .value
            .object;
        vfs.write(&context, new, 0, b"hello", WriteStability::FileSync)
            .await
            .unwrap();
        assert_eq!(std::fs::read(root.path().join("new")).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn invalid_names_stale_replaced_objects_and_cross_export_keys() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"safe").unwrap();
        let vfs = Export::open(root.path(), true).unwrap();
        let c = context();
        for name in [b"..".as_slice(), b".", b"x\\y", b"C:"] {
            assert!(
                vfs.lookup(&c, vfs.root(), &NfsName::new(name.to_vec()).unwrap())
                    .await
                    .is_err()
            );
        }
        let name = NfsName::new(b"file".to_vec()).unwrap();
        let key = vfs.lookup(&c, vfs.root(), &name).await.unwrap().object;
        std::fs::rename(root.path().join("file"), root.path().join("old")).unwrap();
        std::fs::write(root.path().join("file"), b"replacement").unwrap();
        assert_eq!(vfs.getattr(&c, key).await, Err(NfsError::Stale));
        let other = Export::open(root.path(), true).unwrap();
        assert_eq!(other.getattr(&c, key).await, Err(NfsError::Stale));
        let current = vfs.lookup(&c, vfs.root(), &name).await.unwrap().object;
        assert_ne!(current, key);
        vfs.remove(&c, vfs.root(), &name).await.unwrap();
        assert_eq!(vfs.getattr(&c, current).await, Err(NfsError::Stale));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_raw_names_and_size_limits() {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"outside").unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();
        symlink(outside.path().join("secret"), root.path().join("secret")).unwrap();
        let v = Export::open(root.path(), true).unwrap();
        let c = context();
        for name in [b"link".as_slice(), b"secret"] {
            assert!(
                v.lookup(&c, v.root(), &NfsName::new(name.to_vec()).unwrap())
                    .await
                    .is_err()
            );
        }
        let raw = vec![b'x', 0xff];
        std::fs::write(root.path().join(OsString::from_vec(raw.clone())), b"raw").unwrap();
        let key = v
            .lookup(&c, v.root(), &NfsName::new(raw.clone()).unwrap())
            .await
            .unwrap()
            .object;
        assert_eq!(v.read(&c, key, 0, 3).await.unwrap().data, b"raw");
        let page = v.readdir(&c, v.root(), 0, [0; 8], 128).await.unwrap();
        assert!(page.entries.iter().any(|e| e.name.as_bytes() == raw));
        assert_eq!(
            v.write(&c, key, MAX_BYTES, b"x", WriteStability::FileSync)
                .await,
            Err(NfsError::FileTooLarge)
        );
        assert_eq!(
            std::fs::read(outside.path().join("secret")).unwrap(),
            b"outside"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires C compiler and libnfs development library; run explicitly for interoperability evidence"]
    fn independent_libnfs_and_owned_stop() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let build = tempfile::tempdir().unwrap();
        let client = build.path().join("client");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = if manifest
            .join("docs/parity-evidence/E03/nfs-libnfs-client.c")
            .exists()
        {
            manifest.join("docs/parity-evidence/E03/nfs-libnfs-client.c")
        } else {
            manifest.join("../docs/parity-evidence/E03/nfs-libnfs-client.c")
        };
        let compiled = std::process::Command::new("cc")
            .arg(&source)
            .args(["-lnfs", "-o"])
            .arg(&client)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        for writable in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("existing"), b"original").unwrap();
            let listener =
                Listener::bind("127.0.0.1:0".parse().unwrap(), root.path(), writable).unwrap();
            let address = listener.address().unwrap();
            assert!(Listener::bind(address, root.path(), false).is_err());
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            let worker =
                std::thread::spawn(move || listener.serve(|| signal.load(Ordering::Acquire)));
            let result = std::process::Command::new(&client)
                .args([
                    address.port().to_string(),
                    if writable { "rw" } else { "ro" }.into(),
                ])
                .output()
                .unwrap();
            stop.store(true, Ordering::Release);
            worker.join().unwrap().unwrap();
            assert!(
                result.status.success(),
                "{} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                std::fs::read(root.path().join("existing")).unwrap(),
                if writable { b"updated!" } else { b"original" }
            );
            assert!(
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err()
            );
            std::net::TcpListener::bind(address).unwrap();
        }
    }

    #[tokio::test]
    async fn create_modes_truncate_replacement_cookies_and_quota() {
        let root = tempfile::tempdir().unwrap();
        let v = Export::open(root.path(), true).unwrap();
        let c = context();
        let name = NfsName::new(b"file".to_vec()).unwrap();
        let object = v
            .create(
                &c,
                v.root(),
                &name,
                SetAttributes::default(),
                CreateMode::Exclusive { verifier: [4; 8] },
            )
            .await
            .unwrap()
            .value;
        let repeat = v
            .create(
                &c,
                v.root(),
                &name,
                SetAttributes::default(),
                CreateMode::Exclusive { verifier: [4; 8] },
            )
            .await
            .unwrap()
            .value;
        assert_eq!(repeat.object, object.object);
        assert_eq!(
            v.create(
                &c,
                v.root(),
                &name,
                SetAttributes::default(),
                CreateMode::Exclusive { verifier: [5; 8] }
            )
            .await,
            Err(NfsError::Exists)
        );
        assert_eq!(
            v.create(
                &c,
                v.root(),
                &name,
                SetAttributes::default(),
                CreateMode::Guarded
            )
            .await,
            Err(NfsError::Exists)
        );
        v.write(&c, object.object, 0, b"content", WriteStability::Unstable)
            .await
            .unwrap();
        let repeat = v
            .create(
                &c,
                v.root(),
                &name,
                SetAttributes {
                    size: Some(0),
                    ..Default::default()
                },
                CreateMode::Unchecked,
            )
            .await
            .unwrap();
        assert_eq!(repeat.value.object, object.object);
        assert!(std::fs::read(root.path().join("file")).unwrap().is_empty());
        let dest = NfsName::new(b"dest".to_vec()).unwrap();
        let target = v
            .create(
                &c,
                v.root(),
                &dest,
                SetAttributes::default(),
                CreateMode::Guarded,
            )
            .await
            .unwrap()
            .value;
        v.rename(&c, v.root(), &name, v.root(), &dest)
            .await
            .unwrap();
        assert_eq!(v.getattr(&c, target.object).await, Err(NfsError::Stale));
        v.getattr(&c, object.object).await.unwrap();
        let page = v.readdir(&c, v.root(), 0, [0; 8], 1).await.unwrap();
        assert!(page.eof);
        v.state.lock().unwrap().created = MAX_CREATED;
        assert_eq!(
            v.create(
                &c,
                v.root(),
                &NfsName::new(b"quota".to_vec()).unwrap(),
                SetAttributes::default(),
                CreateMode::Guarded
            )
            .await,
            Err(NfsError::Quota)
        );
    }
    #[test]
    fn cancellation_closes_idle_partial_records_and_only_owned_sockets() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let root = tempfile::tempdir().unwrap();
        let unrelated = TcpListener::bind("127.0.0.1:0").unwrap();
        let unrelated_address = unrelated.local_addr().unwrap();
        let listener = Listener::bind("127.0.0.1:0".parse().unwrap(), root.path(), false).unwrap();
        let address = listener.address().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let worker = std::thread::spawn(move || listener.serve(|| signal.load(Ordering::Acquire)));
        let mut idle = std::net::TcpStream::connect(address).unwrap();
        idle.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        idle.write_all(&[0x80, 0x01]).unwrap();
        let now = std::time::Instant::now();
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
        assert!(now.elapsed() < Duration::from_secs(3));
        let result = idle.read(&mut [0]);
        assert!(
            matches!(result, Ok(0))
                || result.is_err_and(|e| matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                ))
        );
        assert!(std::net::TcpStream::connect(unrelated_address).is_ok());
        assert!(std::net::TcpStream::connect(address).is_err());
    }

    #[test]
    fn raw_wire_traversal_cross_export_invalid_handles_versions_and_record_limits() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        fn opaque(bytes: &[u8]) -> Vec<u8> {
            let mut v = (bytes.len() as u32).to_be_bytes().to_vec();
            v.extend(bytes);
            while v.len() % 4 != 0 {
                v.push(0)
            }
            v
        }
        fn call(
            address: SocketAddr,
            program: u32,
            version: u32,
            procedure: u32,
            args: Vec<u8>,
        ) -> io::Result<Vec<u8>> {
            let mut socket = std::net::TcpStream::connect(address)?;
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request = Vec::new();
            for n in [9, 0, 2, program, version, procedure, 0, 0, 0, 0] {
                request.extend(n.to_be_bytes())
            }
            request.extend(args);
            socket.write_all(&(0x8000_0000u32 | request.len() as u32).to_be_bytes())?;
            socket.write_all(&request)?;
            let mut marker = [0; 4];
            socket.read_exact(&mut marker)?;
            let size = u32::from_be_bytes(marker) & 0x7fff_ffff;
            if size > 128 * 1024 {
                return Err(io::Error::other("unbounded reply"));
            }
            let mut reply = vec![0; size as usize];
            socket.read_exact(&mut reply)?;
            Ok(reply)
        }
        let roots = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let stop = Arc::new(AtomicBool::new(false));
        let mut addresses = Vec::new();
        let mut workers = Vec::new();
        for root in &roots {
            let listener =
                Listener::bind("127.0.0.1:0".parse().unwrap(), root.path(), false).unwrap();
            addresses.push(listener.address().unwrap());
            let signal = stop.clone();
            workers.push(std::thread::spawn(move || {
                listener.serve(|| signal.load(Ordering::Acquire))
            }));
        }
        let result = (|| -> io::Result<_> {
            let mount = call(addresses[0], 100005, 3, 1, opaque(b"/share"))?;
            if mount.len() < 32 || mount[24..28] != [0; 4] {
                return Err(io::Error::other("mount failed"));
            }
            let length = u32::from_be_bytes(mount[28..32].try_into().unwrap()) as usize;
            if mount.len() < 32 + length {
                return Err(io::Error::other("bad handle"));
            }
            let handle = mount[32..32 + length].to_vec();
            let mut lookup = opaque(&handle);
            lookup.extend(opaque(b".."));
            let traversal = call(addresses[0], 100003, 3, 3, lookup)?;
            let cross = call(addresses[1], 100003, 3, 1, opaque(&handle))?;
            let mut invalid = handle;
            invalid[0] ^= 0xff;
            let invalid = call(addresses[0], 100003, 3, 1, opaque(&invalid))?;
            let version = call(addresses[0], 100003, 4, 0, vec![])?;
            let mapper = call(addresses[0], 100000, 2, 3, vec![0; 16])?;
            let mut flood = std::net::TcpStream::connect(addresses[0])?;
            flood.set_read_timeout(Some(Duration::from_secs(2)))?;
            flood.write_all(&0x0100_0000u32.to_be_bytes())?;
            let record = flood.read(&mut [0]);
            Ok((traversal, cross, invalid, version, mapper, record))
        })();
        stop.store(true, Ordering::Release);
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let (traversal, cross, invalid, version, mapper, record) = result.unwrap();
        for reply in [traversal, cross, invalid] {
            assert!(reply.len() >= 28);
            assert_ne!(&reply[24..28], &[0; 4]);
        }
        assert_eq!(&version[20..24], &2u32.to_be_bytes());
        assert_ne!(&mapper[20..24], &[0; 4]);
        assert!(
            matches!(record, Ok(0))
                || record.is_err_and(|e| matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                ))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires C compiler and libnfs development library; run explicitly for interoperability evidence"]
    fn independent_libnfs_hardlink_alias_preserves_outside_bytes() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let build = tempfile::tempdir().unwrap();
        let client = build.path().join("client");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let relative = "docs/parity-evidence/E03/nfs-libnfs-client.c";
        let source = if manifest.join(relative).exists() {
            manifest.join(relative)
        } else {
            manifest.join("..").join(relative)
        };
        let compiled = std::process::Command::new("cc")
            .arg(source)
            .args(["-lnfs", "-o"])
            .arg(&client)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = root.path().join("existing");
        let alias = outside.path().join("secret");
        std::fs::write(&source, b"original").unwrap();
        for writable in [true, false] {
            let listener =
                Listener::bind("127.0.0.1:0".parse().unwrap(), root.path(), writable).unwrap();
            let address = listener.address().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            let worker =
                std::thread::spawn(move || listener.serve(|| signal.load(Ordering::Acquire)));
            let mut command = std::process::Command::new(&client);
            command
                .arg(address.port().to_string())
                .arg(if writable { "hardlink" } else { "ro" });
            if writable {
                command.arg(&source).arg(&alias);
            }
            let result = command.output().unwrap();
            stop.store(true, Ordering::Release);
            worker.join().unwrap().unwrap();
            assert!(
                result.status.success(),
                "{} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(std::fs::read(&alias).unwrap(), b"original");
            assert_eq!(std::fs::read(&source).unwrap(), b"original");
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn hardlink_added_after_lookup_blocks_each_inode_mutation() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = root.path().join("existing");
        let alias = outside.path().join("secret");
        std::fs::write(&source, b"original").unwrap();
        let export = Export::open(root.path(), true).unwrap();
        let c = context();
        let name = NfsName::new(b"existing".to_vec()).unwrap();
        let key = export
            .lookup(&c, export.root(), &name)
            .await
            .unwrap()
            .object;
        std::fs::hard_link(&source, &alias).unwrap();
        let writes = export
            .write(&c, key, 0, b"CHANGED!", WriteStability::FileSync)
            .await;
        let truncate = export
            .setattr(
                &c,
                key,
                SetAttributes {
                    size: Some(0),
                    ..Default::default()
                },
                None,
            )
            .await;
        let create = export
            .create(
                &c,
                export.root(),
                &name,
                SetAttributes {
                    size: Some(0),
                    ..Default::default()
                },
                CreateMode::Unchecked,
            )
            .await;
        assert_eq!(
            (writes, truncate, create),
            (
                Err(NfsError::Access),
                Err(NfsError::Access),
                Err(NfsError::Access)
            )
        );
        assert_eq!(std::fs::read(&alias).unwrap(), b"original");
        assert!(export.lookup(&c, export.root(), &name).await.is_err());
        let readonly = Export::open(root.path(), false).unwrap();
        let key = readonly
            .lookup(&c, readonly.root(), &name)
            .await
            .unwrap()
            .object;
        assert_eq!(
            readonly.read(&c, key, 0, 8).await.unwrap().data,
            b"original"
        );
    }
    fn context() -> RequestContext {
        RequestContext {
            principal: nfsembed::vfs::Principal::Anonymous,
            client_addr: "127.0.0.1:12345".parse().unwrap(),
            export_id: nfsembed::vfs::ExportId(1),
        }
    }
}
