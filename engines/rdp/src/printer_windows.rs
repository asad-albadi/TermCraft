//! Winspool ABI from windows-sys / Winspool.h. Runs only inside owned helper.
use super::Backend;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::GetLastError, Graphics::Printing::*};

pub(super) struct Native {
    handle: PRINTER_HANDLE,
    job: Option<u32>,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn error() -> u32 {
    match unsafe { GetLastError() } {
        5 => 0xc0000022,        // access denied
        2 | 1801 => 0xc0000034, // unknown selected printer
        8 | 14 => 0xc000009a,
        995 => 0xc0000120,
        1460 => 0xc00000b5,
        _ => 0xc0000001,
    }
}
fn ok(value: i32) -> Result<(), u32> {
    if value != 0 { Ok(()) } else { Err(error()) }
}
// Variable-size Winspool records use aligned storage and bounded strings. The
// provider owns all pointers only until this method returns.
fn wide_field(buffer: &[usize], bytes: u32, p: *const u16) -> Result<Vec<u16>, u32> {
    let start = buffer.as_ptr() as usize;
    let end = start + bytes as usize;
    let at = p as usize;
    if at < start || at >= end || at % 2 != 0 {
        return Err(0xc0000001);
    }
    let value = unsafe { std::slice::from_raw_parts(p, (end - at) / 2) };
    let n = value.iter().position(|v| *v == 0).ok_or(0xc0000001u32)?;
    Ok(value[..=n].to_vec())
}
fn buffer(bytes: u32) -> Result<Vec<usize>, u32> {
    if bytes == 0 || bytes > 256 * 1024 {
        return Err(0xc000009a);
    }
    Ok(vec![0; (bytes as usize).div_ceil(size_of::<usize>())])
}
impl Native {
    fn raw_supported(&self) -> Result<(), u32> {
        let mut needed = 0;
        unsafe { GetPrinterW(self.handle, 2, null_mut(), 0, &mut needed) };
        let mut printer = buffer(needed)?;
        let capacity = needed;
        ok(unsafe {
            GetPrinterW(
                self.handle,
                2,
                printer.as_mut_ptr().cast(),
                capacity,
                &mut needed,
            )
        })?;
        if needed > capacity || needed < size_of::<PRINTER_INFO_2W>() as u32 {
            return Err(0xc0000001);
        }
        let info = unsafe { &*printer.as_ptr().cast::<PRINTER_INFO_2W>() };
        // Direct output cannot promise even active-document spool cancellation.
        if info.Attributes & PRINTER_ATTRIBUTE_DIRECT != 0 {
            return Err(0xc00000bb);
        }
        let processor = wide_field(&printer, needed, info.pPrintProcessor)?;
        let server = if info.pServerName.is_null() {
            None
        } else {
            Some(wide_field(&printer, needed, info.pServerName)?)
        };
        let name = server.as_ref().map_or(null(), |s| s.as_ptr());
        let mut count = 0;
        needed = 0;
        unsafe {
            EnumPrintProcessorDatatypesW(
                name,
                processor.as_ptr(),
                1,
                null_mut(),
                0,
                &mut needed,
                &mut count,
            )
        };
        let mut types = buffer(needed)?;
        let capacity = needed;
        ok(unsafe {
            EnumPrintProcessorDatatypesW(
                name,
                processor.as_ptr(),
                1,
                types.as_mut_ptr().cast(),
                capacity,
                &mut needed,
                &mut count,
            )
        })?;
        if needed > capacity
            || (count as usize)
                .checked_mul(size_of::<DATATYPES_INFO_1W>())
                .is_none_or(|n| n > needed as usize)
        {
            return Err(0xc0000001);
        }
        for n in 0..count as usize {
            let data = unsafe { &*types.as_ptr().cast::<DATATYPES_INFO_1W>().add(n) };
            if wide_field(&types, needed, data.pName)? == wide("RAW") {
                return Ok(());
            }
        }
        Err(0xc00000bb)
    }
}
impl Backend for Native {
    fn open(queue: &str) -> Result<Self, u32> {
        let name = wide(queue);
        let mut raw = wide("RAW");
        let defaults = PRINTER_DEFAULTSW {
            pDatatype: raw.as_mut_ptr(),
            pDevMode: null_mut(),
            DesiredAccess: PRINTER_ACCESS_USE,
        };
        let options = PRINTER_OPTIONSW {
            cbSize: size_of::<PRINTER_OPTIONSW>() as u32,
            dwFlags: PRINTER_OPTION_NO_CACHE as u32,
        };
        let mut handle = unsafe { zeroed() };
        ok(unsafe { OpenPrinter2W(name.as_ptr(), &mut handle, &defaults, &options) })?;
        let mut this = Self { handle, job: None };
        this.ready()?;
        Ok(this)
    }
    fn ready(&mut self) -> Result<(), u32> {
        // Level 6 has only one DWORD status: no untrusted returned pointers.
        let mut status = PRINTER_INFO_6 { dwStatus: 0 };
        let mut needed = 0;
        ok(unsafe {
            GetPrinterW(
                self.handle,
                6,
                (&mut status as *mut PRINTER_INFO_6).cast(),
                size_of::<PRINTER_INFO_6>() as u32,
                &mut needed,
            )
        })?;
        let bad = PRINTER_STATUS_ERROR
            | PRINTER_STATUS_PAUSED
            | PRINTER_STATUS_PENDING_DELETION
            | PRINTER_STATUS_OFFLINE
            | PRINTER_STATUS_NOT_AVAILABLE
            | PRINTER_STATUS_SERVER_OFFLINE
            | PRINTER_STATUS_PAPER_JAM
            | PRINTER_STATUS_PAPER_OUT
            | PRINTER_STATUS_NO_TONER
            | PRINTER_STATUS_USER_INTERVENTION;
        if status.dwStatus & bad != 0 {
            return Err(0xc00000a3);
        }
        self.raw_supported()
    }
    fn begin(&mut self, _bytes: u32) -> Result<(), u32> {
        if self.job.is_some() {
            return Err(super::super::INVALID);
        }
        let mut name = wide("TermCraft RDP print job");
        let mut raw = wide("RAW");
        // No output filename, no guessed local driver, no conversion.
        let doc = DOC_INFO_1W {
            pDocName: name.as_mut_ptr(),
            pOutputFile: null_mut(),
            pDatatype: raw.as_mut_ptr(),
        };
        let job = unsafe { StartDocPrinterW(self.handle, 1, &doc) };
        if job == 0 {
            return Err(error());
        }
        self.job = Some(job);
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> Result<(), u32> {
        if self.job.is_none() {
            return Err(super::super::INVALID_HANDLE);
        }
        while !bytes.is_empty() {
            let mut n = 0;
            ok(unsafe {
                WritePrinter(
                    self.handle,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    &mut n,
                )
            })?;
            if n == 0 || n as usize > bytes.len() {
                return Err(0xc0000001);
            }
            bytes = &bytes[n as usize..];
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<(), u32> {
        if self.job.is_none() {
            return Err(super::super::INVALID_HANDLE);
        }
        ok(unsafe { EndDocPrinter(self.handle) })?;
        self.job = None;
        Ok(())
    }
    fn abort(&mut self) -> Result<(), u32> {
        // HPRINTER's current document is the only cancellation authority. Never
        // reopen by name or SetJob on a possibly recycled accepted job ID.
        if self.job.take().is_some() {
            ok(unsafe { AbortPrinter(self.handle) })
        } else {
            Ok(())
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.abort();
        unsafe { ClosePrinter(self.handle) };
    }
}
