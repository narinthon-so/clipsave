//! User-facing feedback when there is no terminal: dialogs, notifications and
//! selecting the new file in the file manager.

pub use imp::{error, info, reveal, saved};

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use windows::core::{w, HSTRING};
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE,
    };

    pub use crate::explorer::reveal;

    fn message_box(text: &str, style: MESSAGEBOX_STYLE) {
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                w!("clipsave"),
                style | MB_OK | MB_SETFOREGROUND,
            )
        };
    }

    pub fn error(msg: &str) {
        message_box(msg, MB_ICONERROR);
    }

    pub fn info(msg: &str) {
        message_box(msg, MB_ICONINFORMATION);
    }

    /// No dialog: the file appears in the open Explorer window.
    pub fn saved(path: &Path) {
        crate::explorer::notify_created(path);
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
