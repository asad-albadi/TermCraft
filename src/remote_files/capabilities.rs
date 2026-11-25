use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteBackendKind {
    #[default]
    Sftp,
    Scp,
}
#[derive(Clone, Copy, Debug)]
pub struct RemoteCapabilities {
    pub browse: bool,
    pub upload_files: bool,
    pub download_files: bool,
    pub exclusive_create: bool,
    pub upload_folders: bool,
    pub download_folders: bool,
    pub edit: bool,
    pub mutate_entries: bool,
    pub recursive_remove: bool,
}
impl RemoteBackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sftp => "SFTP",
            Self::Scp => "SCP compatibility (Python 3 helper)",
        }
    }
    pub fn capabilities(self) -> RemoteCapabilities {
        match self {
            Self::Sftp => RemoteCapabilities {
                browse: true,
                upload_files: true,
                download_files: true,
                exclusive_create: true,
                upload_folders: true,
                download_folders: true,
                edit: true,
                mutate_entries: true,
                recursive_remove: true,
            },
            Self::Scp => RemoteCapabilities {
                browse: true,
                upload_files: true,
                download_files: cfg!(any(unix, windows)),
                exclusive_create: false,
                upload_folders: false,
                download_folders: false,
                edit: false,
                mutate_entries: false,
                recursive_remove: false,
            },
        }
    }
}

impl RemoteCapabilities {
    pub(crate) fn scp(flags: u8) -> Self {
        Self {
            browse: flags & 1 != 0,
            upload_files: flags & 2 != 0,
            download_files: flags & 2 != 0 && cfg!(any(unix, windows)),
            exclusive_create: flags & 4 != 0,
            upload_folders: flags & 64 != 0 && cfg!(any(unix, windows)),
            download_folders: flags & 64 != 0 && cfg!(any(unix, windows)),
            edit: flags & 8 != 0,
            mutate_entries: flags & 16 != 0,
            recursive_remove: flags & 32 != 0,
        }
    }
}
