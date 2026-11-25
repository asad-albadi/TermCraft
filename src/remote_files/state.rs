use super::RemotePath;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
    Other,
}
#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub path: RemotePath,
    pub display_name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct DirectorySnapshot {
    pub connection: u64,
    pub request: u64,
    pub path: RemotePath,
    pub entries: Vec<RemoteEntry>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserFollowMode {
    #[default]
    Manual,
    FollowShell,
}
pub struct BrowserState {
    pub connection: u64,
    pub request: u64,
    pub path: RemotePath,
    pub entries: Vec<RemoteEntry>,
    pub loading: bool,
    pub error: Option<String>,
    pub visible: bool,
    follow_mode: BrowserFollowMode,
    follow_pending: bool,
    has_snapshot: bool,
}
impl BrowserState {
    pub fn new(connection: u64) -> Self {
        Self {
            connection,
            request: 0,
            path: RemotePath::from_bytes(b".".to_vec()).unwrap(),
            entries: Vec::new(),
            loading: true,
            error: None,
            visible: true,
            follow_mode: BrowserFollowMode::Manual,
            follow_pending: false,
            has_snapshot: false,
        }
    }
    /// A background completion must not replace a pending user navigation.
    pub fn refresh_path(&self) -> Option<RemotePath> {
        (!self.loading).then(|| self.path.clone())
    }
    pub fn follow_mode(&self) -> BrowserFollowMode {
        self.follow_mode
    }
    pub fn set_follow_mode(
        &mut self,
        mode: BrowserFollowMode,
    ) -> Option<crate::ssh::ConnectionCommand> {
        self.follow_mode = mode;
        if mode == BrowserFollowMode::Manual && self.follow_pending {
            self.request = self.request.wrapping_add(1);
            self.loading = false;
            self.follow_pending = false;
            if !self.has_snapshot {
                self.loading = true;
                return Some(crate::ssh::ConnectionCommand::List {
                    request: self.request,
                    path: self.path.clone(),
                });
            }
        }
        None
    }
    pub fn on_shell_directory(
        &mut self,
        connection: u64,
        path: RemotePath,
    ) -> Option<crate::ssh::ConnectionCommand> {
        if connection != self.connection
            || self.follow_mode != BrowserFollowMode::FollowShell
            || !path.as_bytes().starts_with(b"/")
        {
            return None;
        }
        if !self.loading && path == self.path {
            return None;
        }
        let request = self.request.wrapping_add(1);
        self.begin(request);
        self.follow_pending = true;
        Some(crate::ssh::ConnectionCommand::List { request, path })
    }
    pub fn begin(&mut self, request: u64) {
        self.request = request;
        self.follow_pending = false;
        self.loading = true;
        self.error = None;
    }
    pub fn apply_snapshot(&mut self, mut snapshot: DirectorySnapshot) -> bool {
        if snapshot.connection != self.connection || snapshot.request != self.request {
            return false;
        }
        snapshot.entries.sort_by(|a, b| {
            (a.kind != EntryKind::Directory)
                .cmp(&(b.kind != EntryKind::Directory))
                .then_with(|| a.path.as_bytes().cmp(b.path.as_bytes()))
        });
        self.path = snapshot.path;
        self.has_snapshot = true;
        self.entries = snapshot.entries;
        self.loading = false;
        self.follow_pending = false;
        self.error = None;
        true
    }
    pub fn fail(&mut self, connection: u64, request: u64, error: String) {
        if connection == self.connection && request == self.request {
            self.loading = false;
            self.follow_pending = false;
            self.error = Some(error);
        }
    }
}
