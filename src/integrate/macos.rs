//! macOS: Finder has no per-user verb registry like Explorer, and Quick Actions
//! only appear for a *selected* folder. `install` puts the binary on a stable
//! path and prints the one-time Quick Action recipe.

use std::fs;
use std::path::PathBuf;

use super::LABEL;

fn bin_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/bin/clipsave"))
}

/// `_agent`: the Ctrl+V agent is Windows-only.
pub fn install(_agent: bool) -> Result<String, String> {
    let src = std::env::current_exe().map_err(|e| format!("Cannot locate clipsave: {e}"))?;
    let bin = bin_path()?;
    if fs::canonicalize(&src).ok() != fs::canonicalize(&bin).ok() {
        if let Some(parent) = bin.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        let tmp = bin.with_extension("new");
        fs::copy(&src, &tmp).map_err(|e| format!("Cannot copy to {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &bin).map_err(|e| format!("Cannot move to {}: {e}", bin.display()))?;
    }

    Ok(format!(
        "Installed {bin}\n\n\
         Add a Finder Quick Action (one time):\n\
         1. Open Automator → New Document → Quick Action.\n\
         2. \"Workflow receives current\": folders, in: Finder.\n\
         3. Add \"Run Shell Script\", Pass input: as arguments, and paste:\n\
         \n    for d in \"$@\"; do '{bin}' save --gui \"$d\"; done\n\n\
         4. Save as \"{LABEL}\".\n\
         Then right-click a folder → Quick Actions → {LABEL}.",
        bin = bin.display()
    ))
}

pub fn uninstall() -> Result<String, String> {
    let bin = bin_path()?;
    if bin.exists() {
        fs::remove_file(&bin).map_err(|e| format!("Cannot remove {}: {e}", bin.display()))?;
    }
    Ok(format!(
        "Removed {}.\nDelete the \"{LABEL}\" Quick Action from ~/Library/Services if you made one.",
        bin.display()
    ))
}
