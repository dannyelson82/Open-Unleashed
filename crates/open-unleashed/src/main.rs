//! The `open-unleashed` executable: the engine lives in the library (`lib.rs`).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    open_unleashed::main();
}
