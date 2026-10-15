//! Windowless build of clipsave for the Explorer context menu: no console
//! window flashes up, and messages are shown in dialogs instead.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    std::process::exit(clipsave::run(std::env::args_os().skip(1), true));
}
