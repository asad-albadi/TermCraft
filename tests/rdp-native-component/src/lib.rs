#![allow(dead_code, unused_imports)]
#[path = "../../../src/desktop/options.rs"]
mod rdp_options;
mod drives;

#[cfg(test)]
#[path = "../../../engines/rdp/src/serial_windows_control.rs"]
mod windows_serial_control_tests;
