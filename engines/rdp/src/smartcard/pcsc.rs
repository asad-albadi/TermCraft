//! Linux pcsc-lite ABI from PCSC/{wintypes,pcsclite,winscard}.h.
//! Windows and macOS use different native types and are deliberately excluded.
use libc::{c_char, c_long, c_ulong, c_void};
use std::{
    ffi::{CStr, CString},
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    ptr,
};
use zeroize::Zeroizing;

pub(super) type Handle = c_long;
type Dword = c_ulong;
type Long = c_long;
pub(super) const ATR_MAX: usize = 33; // pcsc-lite MAX_ATR_SIZE.
pub(super) const INSUFFICIENT: u32 = 0x80100008;
pub(super) const NO_READERS: u32 = 0x8010002e;

#[repr(C)]
struct Pci {
    protocol: Dword,
    length: Dword,
}

#[repr(C)]
struct ReaderState {
    reader: *const c_char,
    user_data: *mut c_void,
    current: Dword,
    event: Dword,
    atr_len: Dword,
    atr: [u8; ATR_MAX],
}

pub(super) struct CardStatus {
    pub state: u32,
    pub protocol: u32,
    pub atr: [u8; ATR_MAX],
    pub atr_len: usize,
}

pub(super) struct WatchStatus {
    pub event: u32,
    pub atr: [u8; ATR_MAX],
    pub atr_len: usize,
}

struct Library(*mut c_void);
impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: This is the owned handle from dlopen, closed exactly once.
        unsafe { libc::dlclose(self.0) };
    }
}
impl Library {
    fn symbol(&self, name: &CStr) -> Result<*mut c_void, ()> {
        // SAFETY: The handle is live and the symbol name is NUL terminated.
        let symbol = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        if symbol.is_null() {
            Err(())
        } else {
            Ok(symbol)
        }
    }
}

pub(super) struct Api {
    _library: Library,
    establish: unsafe extern "C" fn(Dword, *const c_void, *const c_void, *mut Handle) -> Long,
    release: unsafe extern "C" fn(Handle) -> Long,
    valid: unsafe extern "C" fn(Handle) -> Long,
    list: unsafe extern "C" fn(Handle, *const c_char, *mut c_char, *mut Dword) -> Long,
    connect:
        unsafe extern "C" fn(Handle, *const c_char, Dword, Dword, *mut Handle, *mut Dword) -> Long,
    disconnect: unsafe extern "C" fn(Handle, Dword) -> Long,
    cancel: unsafe extern "C" fn(Handle) -> Long,
    begin: unsafe extern "C" fn(Handle) -> Long,
    end: unsafe extern "C" fn(Handle, Dword) -> Long,
    status: unsafe extern "C" fn(
        Handle,
        *mut c_char,
        *mut Dword,
        *mut Dword,
        *mut Dword,
        *mut u8,
        *mut Dword,
    ) -> Long,
    watch: unsafe extern "C" fn(Handle, Dword, *mut ReaderState, Dword) -> Long,
    transmit: unsafe extern "C" fn(
        Handle,
        *const Pci,
        *const u8,
        Dword,
        *mut Pci,
        *mut u8,
        *mut Dword,
    ) -> Long,
}

fn result(code: Long) -> Result<(), u32> {
    match code as u32 {
        0 => Ok(()),
        // pcsc-lite's legacy number for SCARD_E_UNSUPPORTED_FEATURE.
        0x8010001f => Err(0x80100022),
        code => Err(code),
    }
}

fn system_library() -> Option<PathBuf> {
    // The platform package owns these system directories. Include the Debian
    // multiarch locations and the system locations used by RPM and Arch systems.
    let multiarch = match std::env::consts::ARCH {
        "x86_64" => Some("x86_64-linux-gnu"),
        "x86" => Some("i386-linux-gnu"),
        "aarch64" => Some("aarch64-linux-gnu"),
        "arm" => Some("arm-linux-gnueabihf"),
        "riscv64" => Some("riscv64-linux-gnu"),
        "powerpc64" if cfg!(target_endian = "little") => Some("powerpc64le-linux-gnu"),
        "powerpc64" => Some("powerpc64-linux-gnu"),
        "s390x" => Some("s390x-linux-gnu"),
        _ => None,
    };
    let system = ["/usr/lib64", "/usr/lib", "/lib64", "/lib"]
        .into_iter()
        .map(PathBuf::from);
    let multiarch = multiarch.into_iter().flat_map(|arch| {
        [
            Path::new("/usr/lib").join(arch),
            Path::new("/lib").join(arch),
        ]
    });
    multiarch.chain(system).find_map(|directory| {
        let entry = directory.join("libpcsclite.so.1");
        let path = entry.canonicalize().ok()?;
        let metadata = path.metadata().ok()?;
        (trusted_system_path(&entry) && trusted_system_path(&path) && metadata.is_file())
            .then_some(path)
    })
}

fn trusted_system_path(path: &Path) -> bool {
    path.ancestors().all(|part| {
        fs::symlink_metadata(part).is_ok_and(|metadata| {
            metadata.uid() == 0 && (metadata.is_symlink() || metadata.mode() & 0o022 == 0)
        })
    })
}

impl Api {
    pub fn open() -> Result<Self, ()> {
        Self::open_file(&system_library().ok_or(())?)
    }

    pub(super) fn open_file(path: &Path) -> Result<Self, ()> {
        if !path.is_absolute() {
            return Err(());
        }
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| ())?;
        // SAFETY: This loads the NUL-terminated absolute path selected by the worker.
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(());
        }
        let library = Library(handle);
        macro_rules! symbol {
            ($name:literal) => {{
                let address = library.symbol(
                    CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes()).map_err(|_| ())?,
                )?;
                // SAFETY: Each fixed symbol from winscard.h is paired with its Linux C ABI
                // signature in Api. The owned library lives as long as these pointers.
                unsafe { std::mem::transmute(address) }
            }};
        }
        Ok(Self {
            establish: symbol!("SCardEstablishContext"),
            release: symbol!("SCardReleaseContext"),
            valid: symbol!("SCardIsValidContext"),
            list: symbol!("SCardListReaders"),
            connect: symbol!("SCardConnect"),
            disconnect: symbol!("SCardDisconnect"),
            cancel: symbol!("SCardCancel"),
            begin: symbol!("SCardBeginTransaction"),
            end: symbol!("SCardEndTransaction"),
            status: symbol!("SCardStatus"),
            watch: symbol!("SCardGetStatusChange"),
            transmit: symbol!("SCardTransmit"),
            _library: library,
        })
    }

    pub fn establish(&self) -> Result<Handle, u32> {
        let mut context = 0;
        // SAFETY: Reserved pointers are null and context points to writable native storage.
        result(unsafe { (self.establish)(0, ptr::null(), ptr::null(), &mut context) })?;
        Ok(context)
    }

    pub fn release(&self, context: Handle) -> Result<(), u32> {
        // SAFETY: Contexts are held and validated by the session.
        result(unsafe { (self.release)(context) })
    }

    pub fn valid(&self, context: Handle) -> Result<(), u32> {
        // SAFETY: Contexts are held and validated by the session.
        result(unsafe { (self.valid)(context) })
    }

    pub fn cancel(&self, context: Handle) -> Result<(), u32> {
        // SAFETY: Cancellation is only for the specific context owned by this session.
        result(unsafe { (self.cancel)(context) })
    }

    pub fn contains_reader(&self, context: Handle, selected: &CStr) -> Result<bool, u32> {
        let mut readers = vec![0u8; 16 * 1024];
        let mut len = readers.len() as Dword;
        // SAFETY: Writable buffer has len bytes and the group filter is null.
        result(unsafe {
            (self.list)(context, ptr::null(), readers.as_mut_ptr().cast(), &mut len)
        })?;
        let readers = readers.get(..len as usize).ok_or(INSUFFICIENT)?;
        if !readers.ends_with(&[0, 0]) {
            return Err(NO_READERS);
        }
        Ok(readers
            .split(|&byte| byte == 0)
            .any(|name| name == selected.to_bytes()))
    }

    pub fn connect(
        &self,
        context: Handle,
        reader: &CStr,
        share: u32,
        protocols: u32,
    ) -> Result<(Handle, u32), u32> {
        let (mut card, mut protocol) = (0, 0);
        // SAFETY: Reader is NUL terminated and both output parameters have native storage.
        result(unsafe {
            (self.connect)(
                context,
                reader.as_ptr(),
                share.into(),
                protocols.into(),
                &mut card,
                &mut protocol,
            )
        })?;
        Ok((card, protocol as u32))
    }

    pub fn disconnect(&self, card: Handle, disposition: u32) -> Result<(), u32> {
        // SAFETY: The session owns the card and checked the disposition.
        result(unsafe { (self.disconnect)(card, disposition.into()) })
    }

    pub fn begin(&self, card: Handle) -> Result<(), u32> {
        // SAFETY: The session owns the card handle.
        result(unsafe { (self.begin)(card) })
    }

    pub fn end(&self, card: Handle, disposition: u32) -> Result<(), u32> {
        // SAFETY: The session owns the card and checked the disposition.
        result(unsafe { (self.end)(card, disposition.into()) })
    }

    pub fn status(&self, card: Handle) -> Result<CardStatus, u32> {
        let mut reader = [0u8; 256];
        let mut reader_len = reader.len() as Dword;
        let (mut state, mut protocol) = (0, 0);
        let mut atr = [0; ATR_MAX];
        let mut atr_len = ATR_MAX as Dword;
        // SAFETY: All native output storage is valid and both array lengths are exact.
        result(unsafe {
            (self.status)(
                card,
                reader.as_mut_ptr().cast(),
                &mut reader_len,
                &mut state,
                &mut protocol,
                atr.as_mut_ptr(),
                &mut atr_len,
            )
        })?;
        if atr_len as usize > ATR_MAX || reader_len as usize > reader.len() {
            return Err(INSUFFICIENT);
        }
        // pcsc-lite returns status bits, while MS-RDPESC carries the Windows enumeration.
        let state = match state {
            flags if flags & 64 != 0 => 6, // SCARD_SPECIFIC
            flags if flags & 32 != 0 => 5, // SCARD_NEGOTIABLE
            flags if flags & 16 != 0 => 4, // SCARD_POWERED
            flags if flags & 8 != 0 => 3,  // SCARD_SWALLOWED
            flags if flags & 4 != 0 => 2,  // SCARD_PRESENT
            flags if flags & 2 != 0 => 1,  // SCARD_ABSENT
            _ => 0,                        // SCARD_UNKNOWN
        };
        Ok(CardStatus {
            state,
            protocol: protocol as u32,
            atr,
            atr_len: atr_len as usize,
        })
    }

    pub fn watch(&self, context: Handle, reader: &CStr, current: u32) -> Result<WatchStatus, u32> {
        let mut state = ReaderState {
            reader: reader.as_ptr(),
            user_data: ptr::null_mut(),
            current: current.into(),
            event: 0,
            atr_len: 0,
            atr: [0; ATR_MAX],
        };
        // SAFETY: state is a single correctly sized Linux SCARD_READERSTATE.
        // All watches are polled so other contexts and cancel requests can progress.
        result(unsafe { (self.watch)(context, 0, &mut state, 1) })?;
        if state.atr_len as usize > ATR_MAX {
            return Err(INSUFFICIENT);
        }
        Ok(WatchStatus {
            event: state.event as u32,
            atr: state.atr,
            atr_len: state.atr_len as usize,
        })
    }

    pub fn transmit(
        &self,
        card: Handle,
        protocol: u32,
        apdu: &[u8],
        recv_protocol: bool,
        capacity: usize,
    ) -> Result<(u32, Zeroizing<Vec<u8>>), u32> {
        let mut response = Zeroizing::new(vec![0u8; capacity]);
        let mut len = capacity as Dword;
        let send = Pci {
            protocol: protocol.into(),
            length: size_of::<Pci>() as Dword,
        };
        let mut receive = Pci {
            protocol: 0,
            length: size_of::<Pci>() as Dword,
        };
        let receive_ptr = if recv_protocol {
            &mut receive
        } else {
            ptr::null_mut()
        };
        // SAFETY: Both PCI structs have the native layout, base T=0/T=1 use no extra PCI
        // data, and the request and response buffers have their declared lengths.
        result(unsafe {
            (self.transmit)(
                card,
                &send,
                apdu.as_ptr(),
                apdu.len() as Dword,
                receive_ptr,
                response.as_mut_ptr(),
                &mut len,
            )
        })?;
        if len as usize > capacity || (recv_protocol && receive.length != size_of::<Pci>() as Dword)
        {
            return Err(INSUFFICIENT);
        }
        // Keep the freshly allocated capacity covered by Zeroizing even on error.
        response.truncate(len as usize);
        Ok((receive.protocol as u32, response))
    }
}
