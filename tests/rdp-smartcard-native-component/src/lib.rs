#![allow(dead_code, unused_imports)]
#[cfg(not(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows")))]
#[path = "../../../engines/rdp/src/smartcard.rs"]
mod smartcard;
#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
#[path = "smartcard_fixture.rs"]
mod smartcard;

#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
pub fn fixture_entry() -> Result<(), ()> {
    smartcard::fixture_entry()
}

#[cfg(test)]
#[path = "../../../engines/rdp/src/smartcard/process_command.rs"]
mod process_command;

#[cfg(any(feature = "pcsc-test-macos", feature = "pcsc-test-windows"))]
pub fn fixture_parent() -> ! {
    smartcard::fixture_parent()
}
