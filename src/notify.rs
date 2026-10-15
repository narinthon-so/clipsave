//! User-facing feedback when there is no terminal: dialogs, notifications and
//! selecting the new file in the file manager.

pub use imp::{error, info, reveal, saved};

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::thread::sleep;
    use std::time::Duration;

    use windows::core::{w, Interface, HSTRING};
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IDispatch,
        IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        IFolderView, ILFree, IPersistFolder2, IShellBrowser, IShellFolder, IShellView,
        IShellWindows, SHChangeNotify, SHGetNameFromIDList, SID_STopLevelBrowser, ShellWindows,
        SHCNE_CREATE, SHCNF_FLUSH, SHCNF_PATHW, SIGDN_FILESYSPATH, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE,
    };

    // SVSI_* flags for IShellView::SelectItem: select only this item, scroll
    // to it, focus it and open the rename box.
    const SVSI_EDIT: u32 = 0x03;
    const SVSI_DESELECTOTHERS: u32 = 0x04;
    const SVSI_ENSUREVISIBLE: u32 = 0x08;
    const SVSI_FOCUSED: u32 = 0x10;
    const CSIDL_DESKTOP: i32 = 0;

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
        unsafe { notify_created(path) };
    }

    /// Selects the new file in the Explorer window (or desktop) that already
    /// shows its folder and opens the rename box, so the user can type a name
    /// straight away. Opens no new window: `SHOpenFolderAndSelectItems` would
    /// open a second one on Windows 11 instead of reusing the open one.
    pub fn reveal(path: &Path) {
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            notify_created(path);
            let _ = select_in_open_view(path);
            if com.is_ok() {
                CoUninitialize();
            }
        }
    }

    /// Tells the shell the file exists, so open views list it right away.
    unsafe fn notify_created(path: &Path) {
        let wide = HSTRING::from(path.as_os_str());
        SHChangeNotify(
            SHCNE_CREATE,
            SHCNF_PATHW | SHCNF_FLUSH,
            Some(wide.as_ptr().cast()),
            None,
        );
    }

    /// Owns a PIDL allocated by the shell.
    struct Pidl(*mut ITEMIDLIST);

    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { ILFree(Some(self.0)) };
        }
    }

    unsafe fn select_in_open_view(path: &Path) -> windows::core::Result<()> {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Ok(());
        };
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;

        // Every Explorer window/tab, then the desktop (not part of the list).
        let mut candidates: Vec<IDispatch> = Vec::new();
        for i in 0..windows.Count()? {
            if let Ok(d) = windows.Item(&VARIANT::from(i)) {
                candidates.push(d);
            }
        }
        let mut hwnd = 0;
        if let Ok(d) = windows.FindWindowSW(
            &VARIANT::from(CSIDL_DESKTOP),
            &VARIANT::default(),
            SWC_DESKTOP,
            &mut hwnd,
            SWFO_NEEDDISPATCH,
        ) {
            candidates.push(d);
        }

        let name = HSTRING::from(name);
        for dispatch in candidates {
            let Some((view, folder)) = view_of(&dispatch, dir) else {
                continue;
            };
            // Parse the name through the view's own folder: an ID list built
            // from the full path has a different form and is not recognised.
            let mut child = std::ptr::null_mut();
            if folder
                .ParseDisplayName(
                    HWND::default(),
                    None,
                    &name,
                    None,
                    &mut child,
                    std::ptr::null_mut(),
                )
                .is_err()
            {
                continue;
            }
            let child = Pidl(child);
            let Ok(folder_view) = view.cast::<IFolderView>() else {
                continue;
            };
            // The view may not list the new file yet: retry for up to 2 s.
            for _ in 0..20 {
                let flags = SVSI_EDIT | SVSI_DESELECTOTHERS | SVSI_ENSUREVISIBLE | SVSI_FOCUSED;
                if view.SelectItem(child.0, flags).is_ok()
                    && is_focused(&folder_view, &folder, &child)
                {
                    break;
                }
                sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    }

    /// The window's active view and its folder, if it is showing `dir`.
    unsafe fn view_of(dispatch: &IDispatch, dir: &Path) -> Option<(IShellView, IShellFolder)> {
        let browser: IShellBrowser = dispatch
            .cast::<IServiceProvider>()
            .ok()?
            .QueryService(&SID_STopLevelBrowser)
            .ok()?;
        let view = browser.QueryActiveShellView().ok()?;
        let folder_view = view.cast::<IFolderView>().ok()?;
        let current = Pidl(
            folder_view
                .GetFolder::<IPersistFolder2>()
                .ok()?
                .GetCurFolder()
                .ok()?,
        );
        let shown = SHGetNameFromIDList(current.0, SIGDN_FILESYSPATH).ok()?;
        let shown_path = shown.to_string();
        CoTaskMemFree(Some(shown.0 as *const _));
        same_path(&shown_path.ok()?, dir)
            .then(|| Some((view, folder_view.GetFolder::<IShellFolder>().ok()?)))?
    }

    /// SelectItem succeeds silently while the view has not picked the file up
    /// yet, so check that the focused item really is the new file.
    unsafe fn is_focused(view: &IFolderView, folder: &IShellFolder, child: &Pidl) -> bool {
        let Ok(index) = view.GetFocusedItem() else {
            return false;
        };
        let Ok(item) = view.Item(index) else {
            return false;
        };
        let item = Pidl(item);
        folder.CompareIDs(LPARAM(0), item.0, child.0).0 & 0xFFFF == 0
    }

    fn same_path(a: &str, b: &Path) -> bool {
        let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
        norm(a) == norm(&b.to_string_lossy())
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
