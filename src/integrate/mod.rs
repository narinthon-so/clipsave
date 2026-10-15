//! `clipsave install` / `uninstall`: copy the binary to a stable per-user
//! location and hook it into the file manager's right-click menu.
//! Never needs admin/root rights.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{install, uninstall};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{install, uninstall};

#[cfg(not(any(windows, target_os = "macos")))]
mod linux;
#[cfg(not(any(windows, target_os = "macos")))]
pub use linux::{install, uninstall};

/// Menu label shared by every platform.
pub const LABEL: &str = "Paste image here";
