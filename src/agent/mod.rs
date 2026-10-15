//! `clipsave agent`: a small background process that makes Ctrl+V in
//! Explorer save the clipboard image (Windows only).

#[cfg_attr(not(windows), allow(dead_code))]
mod rules;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{run, start, stop};

#[cfg(not(windows))]
pub fn run() -> Result<String, String> {
    Err("The Ctrl+V agent is only available on Windows.".into())
}

#[cfg(not(windows))]
pub fn stop() -> bool {
    false
}
