//! User-facing feedback when there is no terminal: dialogs, notifications and
//! selecting the new file in the file manager.

pub use imp::{error, info, reveal, saved};

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::null;

    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
    };

    /// OFASI_EDIT: select the item and put it in rename mode.
    const OFASI_EDIT: u32 = 0x0001;

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn message_box(text: &str, flags: u32) {
        let (text, title) = (wide(text), wide("clipsave"));
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                flags | MB_OK | MB_SETFOREGROUND,
            )
        };
    }

    pub fn error(msg: &str) {
        message_box(msg, MB_ICONERROR);
    }

    pub fn info(msg: &str) {
        message_box(msg, MB_ICONINFORMATION);
    }

    /// Nothing to show: the file appears in the open Explorer window.
    pub fn saved(_path: &Path) {}

    /// Selects the new file in its (already open) Explorer window and starts
    /// renaming it, so the user can type a name straight away.
    pub fn reveal(path: &Path) {
        let wpath = wide(path);
        unsafe {
            let com = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
            let pidl = ILCreateFromPathW(wpath.as_ptr());
            if !pidl.is_null() {
                SHOpenFolderAndSelectItems(pidl, 0, null(), OFASI_EDIT);
                ILFree(pidl);
            }
            if com >= 0 {
                CoUninitialize();
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::Path;
    use std::process::Command;

    /// AppleScript string literal.
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }

    fn osascript(script: String) {
        let _ = Command::new("osascript").arg("-e").arg(script).status();
    }

    pub fn error(msg: &str) {
        osascript(format!(
            "display alert \"clipsave\" message {} as critical",
            quote(msg)
        ));
    }

    pub fn info(msg: &str) {
        osascript(format!("display alert \"clipsave\" message {}", quote(msg)));
    }

    pub fn saved(path: &Path) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        osascript(format!(
            "display notification {} with title \"clipsave\"",
            quote(&format!("Saved {name}"))
        ));
    }

    pub fn reveal(path: &Path) {
        let _ = Command::new("open").arg("-R").arg(path).status();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::path::Path;
    use std::process::Command;

    fn notify_send(urgency: &str, body: &str) {
        let _ = Command::new("notify-send")
            .args([
                "--app-name=clipsave",
                "--icon=image-x-generic",
                "--urgency",
                urgency,
                "clipsave",
                body,
            ])
            .status();
    }

    pub fn error(msg: &str) {
        notify_send("critical", msg);
    }

    pub fn info(msg: &str) {
        notify_send("normal", msg);
    }

    pub fn saved(path: &Path) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        notify_send("low", &format!("Saved {name}"));
    }

    /// Freedesktop has no portable "select this file"; a notification is the
    /// closest equivalent and the file manager refreshes on its own.
    pub fn reveal(path: &Path) {
        saved(path);
    }
}
