//! Native PC/SC adapters. Exact platform layout: see D02 PRIMARY-REFERENCES.
use std::{
    ffi::{CStr, c_void},
    ptr,
};
use zeroize::Zeroizing;
#[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
pub(super) type Handle = i32;
#[cfg(any(windows, feature = "pcsc-test-windows"))]
pub(super) type Handle = usize;
type Dword = u32;
type Long = i32;
#[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
pub(super) const ATR_MAX: usize = 33;
#[cfg(any(windows, feature = "pcsc-test-windows"))]
pub(super) const ATR_MAX: usize = 36;
pub(super) const INSUFFICIENT: u32 = 0x80100008;
pub(super) const NO_READERS: u32 = 0x8010002e;

#[cfg_attr(any(target_os = "macos", feature = "pcsc-test-macos"), repr(C, packed))]
#[cfg_attr(any(windows, feature = "pcsc-test-windows"), repr(C))]
struct Pci {
    protocol: Dword,
    length: Dword,
}

#[cfg_attr(any(target_os = "macos", feature = "pcsc-test-macos"), repr(C, packed))]
#[cfg_attr(any(windows, feature = "pcsc-test-windows"), repr(C))]
struct ReaderState {
    reader: *const NativeChar,
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

#[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
type NativeChar = std::ffi::c_char;
#[cfg(any(windows, feature = "pcsc-test-windows"))]
type NativeChar = u16;
#[cfg(all(
    target_os = "macos",
    not(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))
))]
#[path = "library_macos.rs"]
mod library;
#[cfg(all(
    windows,
    not(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))
))]
#[path = "library_windows.rs"]
mod library;
#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
mod library {
    pub(super) struct Library;
    impl Library {
        pub(super) fn open() -> Result<Self, ()> {
            Err(())
        }
        pub(super) fn symbol(&self, _: &[u8]) -> Result<*mut std::ffi::c_void, ()> {
            Err(())
        }
    }
}
use library::Library;

fn reader_units(reader: &CStr) -> Result<Vec<NativeChar>, u32> {
    #[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
    {
        Ok(reader
            .to_bytes_with_nul()
            .iter()
            .map(|&v| v as NativeChar)
            .collect())
    }
    #[cfg(any(windows, feature = "pcsc-test-windows"))]
    {
        Ok(reader
            .to_str()
            .map_err(|_| 0x80100004u32)?
            .encode_utf16()
            .chain([0])
            .collect())
    }
}

pub(super) struct Api {
    _library: Library,
    establish: unsafe extern "system" fn(Dword, *const c_void, *const c_void, *mut Handle) -> Long,
    release: unsafe extern "system" fn(Handle) -> Long,
    valid: unsafe extern "system" fn(Handle) -> Long,
    list: unsafe extern "system" fn(Handle, *const NativeChar, *mut NativeChar, *mut Dword) -> Long,
    connect: unsafe extern "system" fn(
        Handle,
        *const NativeChar,
        Dword,
        Dword,
        *mut Handle,
        *mut Dword,
    ) -> Long,
    disconnect: unsafe extern "system" fn(Handle, Dword) -> Long,
    cancel: unsafe extern "system" fn(Handle) -> Long,
    begin: unsafe extern "system" fn(Handle) -> Long,
    end: unsafe extern "system" fn(Handle, Dword) -> Long,
    status: unsafe extern "system" fn(
        Handle,
        *mut NativeChar,
        *mut Dword,
        *mut Dword,
        *mut Dword,
        *mut u8,
        *mut Dword,
    ) -> Long,
    watch: unsafe extern "system" fn(Handle, Dword, *mut ReaderState, Dword) -> Long,
    transmit: unsafe extern "system" fn(
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
        // Apple pcsc-lite legacy SCARD_E_UNSUPPORTED_FEATURE; Windows keeps E_UNEXPECTED.
        #[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
        0x8010001f => Err(0x80100022),
        code => Err(code),
    }
}

impl Api {
    pub fn open() -> Result<Self, ()> {
        let library = Library::open()?;
        macro_rules! symbol {
            ($name:literal) => {{
                let address = library.symbol(concat!($name, "\0").as_bytes())?;
                // SAFETY: Fixed official ABI signature, library retained by Api.
                unsafe { std::mem::transmute(address) }
            }};
        }
        #[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
        let (list, connect, status, watch) = (
            symbol!("SCardListReaders"),
            symbol!("SCardConnect"),
            symbol!("SCardStatus"),
            symbol!("SCardGetStatusChange"),
        );
        #[cfg(any(windows, feature = "pcsc-test-windows"))]
        let (list, connect, status, watch) = (
            symbol!("SCardListReadersW"),
            symbol!("SCardConnectW"),
            symbol!("SCardStatusW"),
            symbol!("SCardGetStatusChangeW"),
        );
        Ok(Self {
            establish: symbol!("SCardEstablishContext"),
            release: symbol!("SCardReleaseContext"),
            valid: symbol!("SCardIsValidContext"),
            list,
            connect,
            disconnect: symbol!("SCardDisconnect"),
            cancel: symbol!("SCardCancel"),
            begin: symbol!("SCardBeginTransaction"),
            end: symbol!("SCardEndTransaction"),
            status,
            watch,
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
        let selected = reader_units(selected)?;
        let mut readers = vec![0 as NativeChar; 16 * 1024];
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
            .any(|name| name == &selected[..selected.len() - 1]))
    }

    pub fn connect(
        &self,
        context: Handle,
        reader: &CStr,
        share: u32,
        protocols: u32,
    ) -> Result<(Handle, u32), u32> {
        let reader = reader_units(reader)?;
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
        let mut reader = [0 as NativeChar; 256];
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
        #[cfg(any(target_os = "macos", feature = "pcsc-test-macos"))]
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
        let reader = reader_units(reader)?;
        let mut state = ReaderState {
            reader: reader.as_ptr(),
            user_data: ptr::null_mut(),
            current: current.into(),
            event: 0,
            atr_len: 0,
            atr: [0; ATR_MAX],
        };
        // SAFETY: state is a single correctly sized platform SCARD_READERSTATE.
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

#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEEN: AtomicUsize = AtomicUsize::new(0);
    const NAME: &str = "Reader 日本語 🗝";
    unsafe extern "system" fn establish(
        _: u32,
        _: *const c_void,
        _: *const c_void,
        out: *mut Handle,
    ) -> i32 {
        static ESTABLISH: AtomicUsize = AtomicUsize::new(0);
        let count = ESTABLISH.fetch_add(1, Ordering::SeqCst);
        if let Some(path) = std::env::var_os("D02_NATIVE_ESTABLISH_STALL") {
            if count == 1 {
                std::fs::write(path, b"entered").unwrap();
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        unsafe {
            *out = 17;
        }
        0
    }
    unsafe extern "system" fn simple(_: Handle) -> i32 {
        0
    }
    unsafe extern "system" fn disposition(_: Handle, _: u32) -> i32 {
        0
    }
    unsafe extern "system" fn list(
        _: Handle,
        _: *const NativeChar,
        out: *mut NativeChar,
        len: *mut u32,
    ) -> i32 {
        let units = reader_units(&std::ffi::CString::new(NAME).unwrap()).unwrap();
        let names: Vec<_> = units.iter().copied().chain([0]).collect();
        unsafe {
            if (*len as usize) < names.len() {
                return INSUFFICIENT as i32;
            }
            std::ptr::copy_nonoverlapping(names.as_ptr(), out, names.len());
            *len = names.len() as u32;
        }
        0
    }
    unsafe extern "system" fn connect(
        _: Handle,
        reader: *const NativeChar,
        _: u32,
        _: u32,
        card: *mut Handle,
        protocol: *mut u32,
    ) -> i32 {
        let units = reader_units(&std::ffi::CString::new(NAME).unwrap()).unwrap();
        let actual = unsafe { std::slice::from_raw_parts(reader, units.len()) };
        assert_eq!(actual, units);
        SEEN.fetch_add(1, Ordering::SeqCst);
        unsafe {
            *card = 91;
            *protocol = 2;
        }
        0
    }
    unsafe extern "system" fn status(
        _: Handle,
        _: *mut NativeChar,
        _: *mut u32,
        state: *mut u32,
        protocol: *mut u32,
        atr: *mut u8,
        len: *mut u32,
    ) -> i32 {
        unsafe {
            *state = if cfg!(feature = "pcsc-test-macos") {
                64
            } else {
                6
            };
            *protocol = 2;
            *atr = 0x3b;
            *len = 1;
        }
        0
    }
    unsafe extern "system" fn watch(
        _: Handle,
        timeout: u32,
        state: *mut ReaderState,
        n: u32,
    ) -> i32 {
        assert_eq!((timeout, n), (0, 1));
        if unsafe { std::ptr::addr_of!((*state).current).read_unaligned() } == 0x20 {
            return 0x8010000au32 as i32;
        }
        unsafe {
            std::ptr::addr_of_mut!((*state).event).write_unaligned(0x22);
            std::ptr::addr_of_mut!((*state).atr_len).write_unaligned(1);
            (*state).atr[0] = 0x3b;
        }
        0
    }
    unsafe extern "system" fn transmit(
        _: Handle,
        send: *const Pci,
        bytes: *const u8,
        len: u32,
        recv: *mut Pci,
        out: *mut u8,
        capacity: *mut u32,
    ) -> i32 {
        if let Some(path) = std::env::var_os("D02_NATIVE_TRANSMIT_STALL") {
            std::fs::write(path, b"entered").unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        unsafe {
            assert_eq!(std::ptr::addr_of!((*send).length).read_unaligned(), 8);
            assert_eq!(
                std::slice::from_raw_parts(bytes, len as usize),
                [0, 0xa4, 4]
            );
            if *capacity < 2 {
                return INSUFFICIENT as i32;
            }
            *out = 0x90;
            *out.add(1) = 0;
            *capacity = 2;
            if !recv.is_null() {
                std::ptr::addr_of_mut!((*recv).protocol).write_unaligned(2);
            }
        }
        0
    }
    pub(super) fn fixture() -> Api {
        Api {
            _library: Library,
            establish,
            release: simple,
            valid: simple,
            list,
            connect,
            disconnect: disposition,
            cancel: simple,
            begin: simple,
            end: disposition,
            status,
            watch,
            transmit,
        }
    }
    #[test]
    fn official_layouts_are_exact() {
        assert_eq!(size_of::<Dword>(), 4);
        assert_eq!(size_of::<Long>(), 4);
        assert_eq!(size_of::<Pci>(), 8);
        assert_eq!(
            std::mem::offset_of!(ReaderState, current),
            2 * size_of::<usize>()
        );
        assert_eq!(
            std::mem::offset_of!(ReaderState, atr),
            2 * size_of::<usize>() + 12
        );
        if cfg!(feature = "pcsc-test-macos") {
            assert_eq!(size_of::<Handle>(), 4);
            assert_eq!(std::mem::align_of::<ReaderState>(), 1);
            assert_eq!(size_of::<ReaderState>(), 2 * size_of::<usize>() + 45);
        } else {
            assert_eq!(size_of::<Handle>(), size_of::<usize>());
            assert_eq!(
                size_of::<ReaderState>(),
                if size_of::<usize>() == 8 { 64 } else { 56 }
            );
        }
    }
    #[test]
    fn callbacks_use_native_unicode_fixed_width_pci_and_state_mapping() {
        let api = fixture();
        let name = std::ffi::CString::new(NAME).unwrap();
        assert_eq!(api.establish(), Ok(17));
        assert_eq!(api.contains_reader(17, &name), Ok(true));
        assert_eq!(api.contains_reader(17, c"Other reader"), Ok(false));
        let (card, protocol) = api.connect(17, &name, 2, 3).unwrap();
        assert_eq!((card, protocol), (91, 2));
        assert_eq!(api.status(card).unwrap().state, 6);
        let watch = api.watch(17, &name, 0).unwrap();
        assert_eq!((watch.event, watch.atr_len), (0x22, 1));
        let (protocol, reply) = api.transmit(card, 2, &[0, 0xa4, 4], true, 2).unwrap();
        assert_eq!(protocol, 2);
        assert_eq!(reply.as_slice(), [0x90, 0]);
        assert_eq!(
            api.transmit(card, 2, &[0, 0xa4, 4], true, 1).unwrap_err(),
            INSUFFICIENT
        );
        assert_eq!(
            result(0x8010001fu32 as i32),
            Err(if cfg!(feature = "pcsc-test-macos") {
                0x80100022
            } else {
                0x8010001f
            })
        );
        assert_eq!(SEEN.load(Ordering::SeqCst), 1);
    }
}

#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
impl Api {
    pub(super) fn fixture() -> Self {
        tests::fixture()
    }
}
