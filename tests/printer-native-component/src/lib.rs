#![allow(dead_code, unused_imports)]
#[path = "../../../src/desktop/options.rs"]
mod rdp_options;
const INVALID: u32 = 0xc000000d;
const UNSUPPORTED: u32 = 0xc00000bb;
const MAX_IO: usize = 64 * 1024;
type DeviceCompletion = (Vec<u8>, Option<&'static str>);
fn u32_at(p: &[u8], at: usize) -> Result<u32, u32> {
    Ok(u32::from_le_bytes(
        p.get(at..at + 4).ok_or(INVALID)?.try_into().unwrap(),
    ))
}
fn word(p: &mut Vec<u8>, n: u32) {
    p.extend(n.to_le_bytes())
}
fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
fn status(_: std::io::Error) -> u32 {
    0xc0000001
}
fn completion_header(p: &[u8], status: u32) -> Vec<u8> {
    let mut out = vec![b'r', b'D', 0x43, 0x49];
    out.extend(&p[4..8]);
    out.extend(&p[12..16]);
    word(&mut out, status);
    out
}
#[path = "../../../engines/rdp/src/printer.rs"]
mod printer;
