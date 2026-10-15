//! Windows Explorer automation through the shell's COM interfaces: which
//! folder a window shows, and selecting a file in a view that is already open.
//!
//! Explorer windows (and, on Windows 11, each of their tabs) are listed by
//! `IShellWindows`; the desktop is found separately with `FindWindowSW`.

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use windows::core::{w, Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IDispatch, IServiceProvider,
    CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IFolderView, ILFree, IPersistFolder2, IShellBrowser, IShellFolder, IShellView, IShellWindows,
    IWebBrowser2, SHChangeNotify, SHGetNameFromIDList, SID_STopLevelBrowser, ShellWindows,
    SHCNE_CREATE, SHCNF_FLUSH, SHCNF_PATHW, SIGDN_FILESYSPATH, SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetClassNameW};

// SVSI_* flags for IShellView::SelectItem: select only this item, scroll to
// it, focus it and open the rename box.
const SVSI_EDIT: u32 = 0x03;
const SVSI_DESELECTOTHERS: u32 = 0x04;
const SVSI_ENSUREVISIBLE: u32 = 0x08;
const SVSI_FOCUSED: u32 = 0x10;
const CSIDL_DESKTOP: i32 = 0;

/// Tells the shell the file exists, so open views list it right away.
pub fn notify_created(path: &Path) {
    let wide = HSTRING::from(path.as_os_str());
    unsafe {
        SHChangeNotify(
            SHCNE_CREATE,
            SHCNF_PATHW | SHCNF_FLUSH,
            Some(wide.as_ptr().cast()),
            None,
        )
    };
}

/// Selects the new file in the Explorer window (or desktop) that already
/// shows its folder and opens the rename box, so the user can type a name
/// straight away. Opens no new window: `SHOpenFolderAndSelectItems` would
/// open a second one on Windows 11 instead of reusing the open one.
pub fn reveal(path: &Path) {
    notify_created(path);
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = select_in_open_view(path);
        if com.is_ok() {
            CoUninitialize();
        }
    }
}

/// The file-system folder shown by a top-level Explorer window (its active
/// tab) or by the desktop. `None` for virtual folders such as Home or This PC.
/// The calling thread must have COM initialised (STA).
pub fn folder_of_window(top: HWND) -> Option<PathBuf> {
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        let browser = if is_desktop(top) {
            browser_of(&desktop(&windows)?)?
        } else {
            explorer_browser(&windows, top)?
        };
        let view = browser.QueryActiveShellView().ok()?;
        view_path(&view.cast::<IFolderView>().ok()?)
    }
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn is_desktop(hwnd: HWND) -> bool {
    matches!(class_name(hwnd).as_str(), "Progman" | "WorkerW")
}

/// The browser of `top`'s active tab. Every tab of a Windows 11 Explorer
/// window reports the same top-level HWND, and inactive tab windows are not
/// hidden (WS_VISIBLE stays set); the active tab is the first
/// `ShellTabWindowClass` child in z-order. With no match, return nothing
/// rather than guess: saving into another tab's folder would be wrong.
unsafe fn explorer_browser(windows: &IShellWindows, top: HWND) -> Option<IShellBrowser> {
    let mut tabs = Vec::new();
    for i in 0..windows.Count().ok()? {
        let Ok(dispatch) = windows.Item(&VARIANT::from(i)) else {
            continue;
        };
        let Ok(web) = dispatch.cast::<IWebBrowser2>() else {
            continue;
        };
        if web.HWND().map(|h| h.0) != Ok(top.0 as isize) {
            continue;
        }
        if let Some(browser) = browser_of(&dispatch) {
            tabs.push(browser);
        }
    }
    if tabs.len() == 1 {
        return tabs.pop(); // Windows 10, or a single tab
    }
    let active = FindWindowExW(Some(top), None, w!("ShellTabWindowClass"), PCWSTR::null()).ok()?;
    tabs.into_iter()
        .find(|b| b.GetWindow().ok() == Some(active))
}

unsafe fn desktop(windows: &IShellWindows) -> Option<IDispatch> {
    let mut hwnd = 0;
    windows
        .FindWindowSW(
            &VARIANT::from(CSIDL_DESKTOP),
            &VARIANT::default(),
            SWC_DESKTOP,
            &mut hwnd,
            SWFO_NEEDDISPATCH,
        )
        .ok()
}

unsafe fn browser_of(dispatch: &IDispatch) -> Option<IShellBrowser> {
    dispatch
        .cast::<IServiceProvider>()
        .ok()?
        .QueryService(&SID_STopLevelBrowser)
        .ok()
}

/// Owns a PIDL allocated by the shell.
struct Pidl(*mut ITEMIDLIST);

impl Drop for Pidl {
    fn drop(&mut self) {
        unsafe { ILFree(Some(self.0)) };
    }
}

/// File-system path of the folder a view shows.
unsafe fn view_path(view: &IFolderView) -> Option<PathBuf> {
    let current = Pidl(
        view.GetFolder::<IPersistFolder2>()
            .ok()?
            .GetCurFolder()
            .ok()?,
    );
    let name = SHGetNameFromIDList(current.0, SIGDN_FILESYSPATH).ok()?;
    let path = name.to_string();
    CoTaskMemFree(Some(name.0 as *const _));
    path.ok().map(PathBuf::from)
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
    candidates.extend(desktop(&windows));

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
            if view.SelectItem(child.0, flags).is_ok() && is_focused(&folder_view, &folder, &child)
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
    let view = browser_of(dispatch)?.QueryActiveShellView().ok()?;
    let folder_view = view.cast::<IFolderView>().ok()?;
    let shown = view_path(&folder_view)?;
    same_path(&shown, dir).then(|| Some((view, folder_view.GetFolder::<IShellFolder>().ok()?)))?
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

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}
