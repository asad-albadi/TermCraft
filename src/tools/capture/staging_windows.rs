use super::*;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptionsExt as _};
use std::{
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    ptr,
};
use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT,
        FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
    },
};
use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING},
    Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo,
        GetFileInformationByHandle, GetFinalPathNameByHandleW, GetVolumeInformationByHandleW,
        SYNCHRONIZE, SetFileInformationByHandle,
    },
    System::IO::IO_STATUS_BLOCK,
};

const SHARE: u32 = FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE;
const DELETE: u32 = 0x0001_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const DIRECTORY_FLAGS: u32 = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT;

pub(crate) struct Stage {
    parent: File,
    parent_path: PathBuf,
    container: File,
    container_name: OsString,
    target: OsString,
    file: Option<File>,
    keep: bool,
    published: bool,
}

impl Stage {
    pub(crate) fn create(path: &Path) -> io::Result<Self> {
        let (parent_path, target) = target(path)?;
        let text = target
            .to_str()
            .ok_or_else(|| io::Error::other("Invalid Windows capture filename"))?;
        if text.contains([':', '/', '\\']) || text.ends_with(['.', ' ']) {
            return Err(io::Error::other("Invalid Windows capture filename"));
        }
        let parent = open_directory(&parent_path)?;
        let capability = Dir::from_std_file(parent.try_clone()?);
        require_acl_volume(&parent)?;
        match capability.symlink_metadata(&target) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Capture output already exists",
                ));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }

        let container_name = random_name()?;
        let container = create_private(&parent, &container_name, true)?;
        let mut stage = Self {
            container,
            parent,
            parent_path,
            container_name,
            target,
            file: None,
            keep: false,
            published: false,
        };
        stage.file = Some(create_private(
            &stage.container,
            STAGED_FILE.as_ref(),
            false,
        )?);
        Ok(stage)
    }

    pub(crate) fn file(&mut self) -> &mut File {
        self.file.as_mut().expect("open capture stage")
    }

    pub(crate) fn publish(&mut self) -> Result<PathBuf, String> {
        match self.publish_anchored() {
            Ok(()) => {
                self.published = true;
                Ok(self.parent_path.join(&self.target))
            }
            Err(err) => {
                self.keep = true;
                Err(match recovery_path(self.file()) {
                    Ok(path) => format!(
                        "Could not save capture output: {err}. Capture retained at {}",
                        path.display()
                    ),
                    Err(_) => format!(
                        "Could not save capture output: {err}. Capture staging could not be located"
                    ),
                })
            }
        }
    }

    fn publish_anchored(&self) -> io::Result<()> {
        if identity(&open_directory(&self.parent_path)?)? != identity(&self.parent)? {
            return Err(io::Error::other(
                "Selected capture directory moved or changed",
            ));
        }
        let capability = Dir::from_std_file(self.parent.try_clone()?);
        if identity(&open_child_directory(&capability, &self.container_name)?)?
            != identity(&self.container)?
        {
            return Err(io::Error::other(
                "Capture staging directory moved or changed",
            ));
        }
        let file = self.file.as_ref().expect("open capture stage");
        crate::platform::windows_rename::rename(file, &self.parent, &self.target, false)
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        if let Some(file) = self.file.take()
            && !self.published
        {
            mark_delete(&file);
        }
        mark_delete(&self.container);
    }
}

fn open_directory(path: &Path) -> io::Result<File> {
    let directory = fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE)
        .custom_flags(DIRECTORY_FLAGS)
        .open(path)?;
    validate_directory(directory)
}

fn open_child_directory(parent: &Dir, name: &OsString) -> io::Result<File> {
    let mut options = cap_std::fs::OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .access_mode(GENERIC_READ)
        .share_mode(SHARE)
        .custom_flags(DIRECTORY_FLAGS);
    validate_directory(parent.open_with(name, &options)?.into_std())
}

fn validate_directory(file: File) -> io::Result<File> {
    let info = file.metadata()?;
    if !info.is_dir() || info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(
            "Expected an ordinary capture output directory",
        ));
    }
    Ok(file)
}

fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

fn require_acl_volume(file: &File) -> io::Result<()> {
    let mut flags = 0;
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle() as HANDLE,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut flags,
            ptr::null_mut(),
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    const FILE_PERSISTENT_ACLS: u32 = 8;
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Choose an output volume that supports private file permissions",
        ));
    }
    Ok(())
}

fn create_private(parent: &File, name: &std::ffi::OsStr, directory: bool) -> io::Result<File> {
    let name: Vec<u16> = name.encode_wide().collect();
    let size = u16::try_from(name.len() * 2)
        .map_err(|_| io::Error::other("Capture staging name exceeds its bound"))?;
    let name = UNICODE_STRING {
        Length: size,
        MaximumLength: size,
        Buffer: name.as_ptr().cast_mut(),
    };
    let owner_acl: Vec<u16> = "D:P(A;;FA;;;OW)".encode_utf16().chain([0]).collect();
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            owner_acl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: &name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: descriptor.cast(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut file = ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    let type_option = if directory {
        FILE_DIRECTORY_FILE
    } else {
        FILE_NON_DIRECTORY_FILE
    };
    let access = GENERIC_READ | DELETE | SYNCHRONIZE | if directory { 0 } else { GENERIC_WRITE };
    // The single-component name is resolved relative to the held directory.
    // The exclusive create installs the ACL and returns the new handle atomically.
    let code = unsafe {
        NtCreateFile(
            &mut file,
            access,
            &attributes,
            &mut status,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            SHARE,
            FILE_CREATE,
            type_option | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            ptr::null(),
            0,
        )
    };
    unsafe { LocalFree(descriptor) };
    if code < 0 {
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(code) } as i32,
        ));
    }
    // NtCreateFile succeeded and transferred ownership of the handle to us.
    Ok(unsafe { File::from_raw_handle(file) })
}

fn mark_delete(file: &File) {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    };
}

fn recovery_path(file: &File) -> io::Result<PathBuf> {
    let mut wide = vec![0u16; 32_768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle() as HANDLE,
            wide.as_mut_ptr(),
            wide.len() as u32,
            0,
        )
    } as usize;
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    if length >= wide.len() {
        return Err(io::Error::other("Capture staging path exceeds its bound"));
    }
    Ok(OsString::from_wide(&wide[..length]).into())
}
