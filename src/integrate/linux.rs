//! Linux/BSD integration: the binary goes to ~/.local/bin and a menu entry is
//! written for every supported file manager found on PATH.
//!
//! | File manager | Entry                                              | Where it shows |
//! |--------------|----------------------------------------------------|----------------|
//! | Nautilus     | ~/.local/share/nautilus/scripts/Paste image here   | Scripts submenu |
//! | Nemo         | ~/.local/share/nemo/actions/clipsave.nemo_action   | folder background |
//! | Dolphin      | ~/.local/share/kio/servicemenus/clipsave.desktop   | Actions submenu |

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::LABEL;

struct Entry {
    manager: &'static str,
    /// Executable names that mean this file manager is installed.
    detect: &'static [&'static str],
    /// Path under $XDG_DATA_HOME (~/.local/share).
    rel_path: &'static str,
    render: fn(&Path) -> String,
}

const ENTRIES: [Entry; 4] = [
    Entry {
        manager: "Nautilus",
        detect: &["nautilus"],
        rel_path: "nautilus/scripts/Paste image here",
        render: nautilus,
    },
    Entry {
        manager: "Nemo",
        detect: &["nemo"],
        rel_path: "nemo/actions/clipsave.nemo_action",
        render: nemo,
    },
    Entry {
        manager: "Dolphin",
        detect: &["dolphin"],
        rel_path: "kio/servicemenus/clipsave.desktop",
        render: dolphin,
    },
    // Plasma 5 still reads the old location.
    Entry {
        manager: "Dolphin (Plasma 5)",
        detect: &["dolphin"],
        rel_path: "kservices5/ServiceMenus/clipsave.desktop",
        render: dolphin,
    },
];

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn data_home() -> Result<PathBuf, String> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(d) if !d.is_empty() => Ok(PathBuf::from(d)),
        _ => Ok(home()?.join(".local/share")),
    }
}

fn bin_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".local/bin/clipsave"))
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// POSIX shell single-quoting.
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Desktop Entry `Exec` quoting: double quotes, with `"` `` ` `` `$` `\` escaped.
/// A literal `%` must be `%%` (field codes).
pub(crate) fn exec_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn nautilus(bin: &Path) -> String {
    // Nautilus runs scripts with the current folder as cwd; a selected folder
    // wins so the script also works when right-clicking a folder icon.
    format!(
        "#!/bin/sh\n\
         # Installed by clipsave — remove with `clipsave uninstall`.\n\
         dir=\"$(pwd)\"\n\
         first=\"$(printf '%s' \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\" | head -n 1)\"\n\
         [ -d \"$first\" ] && dir=\"$first\"\n\
         exec {} save --gui \"$dir\"\n",
        sh_quote(&bin.to_string_lossy())
    )
}

fn nemo(bin: &Path) -> String {
    format!(
        "[Nemo Action]\n\
         # Installed by clipsave — remove with `clipsave uninstall`.\n\
         Name={LABEL}\n\
         Comment=Save the clipboard image as a PNG in this folder\n\
         Exec={} save --gui %P\n\
         Icon-Name=image-x-generic\n\
         Selection=None\n\
         Extensions=any;\n",
        exec_quote(&bin.to_string_lossy())
    )
}

fn dolphin(bin: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         # Installed by clipsave — remove with `clipsave uninstall`.\n\
         Type=Service\n\
         MimeType=inode/directory;\n\
         Actions=clipsavePaste;\n\
         X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n\
         X-KDE-Priority=TopLevel\n\
         \n\
         [Desktop Action clipsavePaste]\n\
         Name={LABEL}\n\
         Icon=image-x-generic\n\
         Exec={} save --gui %f\n",
        exec_quote(&bin.to_string_lossy())
    )
}

fn write_executable(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Cannot chmod {}: {e}", path.display()))
}

pub fn install() -> Result<String, String> {
    let src = std::env::current_exe().map_err(|e| format!("Cannot locate clipsave: {e}"))?;
    let bin = bin_path()?;
    if fs::canonicalize(&src).ok() != fs::canonicalize(&bin).ok() {
        if let Some(parent) = bin.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        // Copy to a temp name and rename, so a running copy is never truncated.
        let tmp = bin.with_extension("new");
        fs::copy(&src, &tmp).map_err(|e| format!("Cannot copy to {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &bin).map_err(|e| format!("Cannot move to {}: {e}", bin.display()))?;
    }

    let data = data_home()?;
    let mut done = Vec::new();
    for entry in &ENTRIES {
        if entry.detect.iter().any(|d| on_path(d)) {
            let path = data.join(entry.rel_path);
            write_executable(&path, &(entry.render)(&bin))?;
            done.push(format!("  {:<20} {}", entry.manager, path.display()));
        }
    }

    let mut msg = format!("Installed {}\n", bin.display());
    if done.is_empty() {
        msg.push_str(
            "\nNo supported file manager found (Nautilus, Nemo, Dolphin).\n\
             Use `clipsave [DIR]` from a terminal, or bind it to a keyboard shortcut.",
        );
    } else {
        msg.push_str(&format!("\nAdded \"{LABEL}\" for:\n{}", done.join("\n")));
        if done.iter().any(|l| l.contains("Nautilus")) {
            msg.push_str("\n\nNautilus: right-click → Scripts → Paste image here.");
        }
    }
    if !on_path("clipsave") {
        msg.push_str("\n\nNote: ~/.local/bin is not on your PATH (only matters for terminal use).");
    }
    Ok(msg)
}

pub fn uninstall() -> Result<String, String> {
    let data = data_home()?;
    let mut removed = Vec::new();
    let paths = ENTRIES
        .iter()
        .map(|e| data.join(e.rel_path))
        .chain(Some(bin_path()?));
    for path in paths {
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Cannot remove {}: {e}", path.display()))?;
            removed.push(format!("  {}", path.display()));
        }
    }
    if removed.is_empty() {
        Ok("Nothing to remove.".into())
    } else {
        Ok(format!("Removed:\n{}", removed.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("/home/a b/it's"), r"'/home/a b/it'\''s'");
        assert_eq!(
            exec_quote("/opt/$x \"y\" 100%"),
            r#""/opt/\$x \"y\" 100%%""#
        );
    }

    #[test]
    fn rendered_entries_reference_the_binary() {
        let bin = Path::new("/home/me/.local/bin/clipsave");
        assert!(nautilus(bin).contains("exec '/home/me/.local/bin/clipsave' save --gui \"$dir\""));
        assert!(nemo(bin).contains("Exec=\"/home/me/.local/bin/clipsave\" save --gui %P"));
        assert!(nemo(bin).contains("Selection=None"));
        assert!(dolphin(bin).contains("Exec=\"/home/me/.local/bin/clipsave\" save --gui %f"));
    }
}
