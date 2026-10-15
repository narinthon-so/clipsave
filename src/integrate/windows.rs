//! Explorer integration through per-user registry verbs (HKCU, no admin).
//!
//! On Windows 11 classic verbs live under "Show more options" (or Shift+F10 /
//! Shift+right-click). A top-level Windows 11 entry needs an IExplorerCommand
//! COM handler in a signed package, which is out of scope here; the Ctrl+V
//! agent (started at login from the Run key) is the one-step alternative.

use std::fs;
use std::path::{Path, PathBuf};

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use super::LABEL;

const BINARIES: [&str; 2] = ["clipsave.exe", "clipsavew.exe"];

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "clipsave";

/// (registry key under HKCU\Software\Classes, menu label, Explorer placeholder, extra flags)
const VERBS: [(&str, &str, &str, &str); 2] = [
    // Right-click on empty space inside a folder or on the desktop.
    (
        r"Software\Classes\Directory\Background\shell\clipsave",
        LABEL,
        "%V",
        " --reveal",
    ),
    // Right-click on a folder icon: save into that folder without opening it.
    (
        r"Software\Classes\Directory\shell\clipsave",
        "Paste image into folder",
        "%1",
        "",
    ),
];

fn install_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(base).join("Programs").join("clipsave"))
}

/// The registry command line. `"%V"` on a drive root arrives as `D:"`; the
/// save command repairs that (see `output::resolve_dir`).
pub(crate) fn command_line(exe: &Path, placeholder: &str, flags: &str) -> String {
    format!("\"{}\" save --gui{flags} \"{placeholder}\"", exe.display())
}

pub fn install(agent: bool) -> Result<String, String> {
    let src_dir = std::env::current_exe()
        .map_err(|e| format!("Cannot locate clipsave.exe: {e}"))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or("Cannot locate the clipsave folder")?;
    let dest = install_dir()?;

    // A running agent locks clipsavew.exe: stop it before copying over it.
    crate::agent::stop();

    // Copy both binaries so the menu keeps working after the download is deleted.
    if !same_dir(&src_dir, &dest) {
        fs::create_dir_all(&dest).map_err(|e| format!("Cannot create {}: {e}", dest.display()))?;
        for bin in BINARIES {
            let from = src_dir.join(bin);
            if !from.is_file() {
                return Err(format!(
                    "{bin} must be next to clipsave.exe ({} is missing)",
                    from.display()
                ));
            }
            fs::copy(&from, dest.join(bin))
                .map_err(|e| format!("Cannot copy {bin} to {}: {e}", dest.display()))?;
        }
    }

    let exe = dest.join("clipsavew.exe");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (key, label, placeholder, flags) in VERBS {
        let reg_err = |e: std::io::Error| format!("Cannot write HKCU\\{key}: {e}");
        let (verb, _) = hkcu.create_subkey(key).map_err(reg_err)?;
        verb.set_value("", &label).map_err(reg_err)?;
        verb.set_value("Icon", &format!("\"{}\",0", exe.display()))
            .map_err(reg_err)?;
        let (cmd, _) = verb.create_subkey("command").map_err(reg_err)?;
        cmd.set_value("", &command_line(&exe, placeholder, flags))
            .map_err(reg_err)?;
    }

    let (run, _) = hkcu
        .create_subkey(RUN_KEY)
        .map_err(|e| format!("Cannot open HKCU\\{RUN_KEY}: {e}"))?;
    let agent_note = if agent {
        run.set_value(RUN_VALUE, &format!("\"{}\" agent", exe.display()))
            .map_err(|e| format!("Cannot write HKCU\\{RUN_KEY}: {e}"))?;
        crate::agent::start(&exe)?;
        "\n\nCtrl+V in Explorer or on the desktop now saves a clipboard image\n\
         (when there is no file to paste). Runs at login; quit from the tray icon."
    } else {
        let _ = run.delete_value(RUN_VALUE);
        ""
    };

    Ok(format!(
        "Installed to {}\n\n\
         Right-click empty space in a folder → Show more options → \"{LABEL}\".\n\
         Tip: Shift+right-click opens the full menu directly.{agent_note}",
        dest.display()
    ))
}

pub fn uninstall() -> Result<String, String> {
    crate::agent::stop();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run) = hkcu.open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE) {
        let _ = run.delete_value(RUN_VALUE);
    }
    for (key, ..) in VERBS {
        match hkcu.delete_subkey_all(key) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot remove HKCU\\{key}: {e}")),
        }
    }

    let dest = install_dir()?;
    let mut left = Vec::new();
    for bin in BINARIES {
        let path = dest.join(bin);
        if path.exists() && fs::remove_file(&path).is_err() {
            left.push(path); // e.g. the running clipsave.exe itself
        }
    }
    let _ = fs::remove_dir(&dest);

    let mut msg = String::from("Removed the right-click menu entries and the Ctrl+V agent.");
    if !left.is_empty() {
        msg.push_str(&format!(
            "\nDelete {} by hand to finish (it is in use).",
            dest.display()
        ));
    }
    Ok(msg)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_everything() {
        let exe = Path::new(r"C:\Users\ผู้ใช้ one\AppData\Local\Programs\clipsave\clipsavew.exe");
        assert_eq!(
            command_line(exe, "%V", " --reveal"),
            r#""C:\Users\ผู้ใช้ one\AppData\Local\Programs\clipsave\clipsavew.exe" save --gui --reveal "%V""#
        );
    }
}
