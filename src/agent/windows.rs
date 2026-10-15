//! The Windows agent: a low-level keyboard hook that turns Ctrl+V in
//! Explorer into "save the clipboard image here".
//!
//! The hook callback only looks at window classes, modifier state and which
//! clipboard formats exist — no COM, no clipboard reads, no file I/O — because
//! Windows silently removes a low-level hook that answers too slowly. The
//! actual work happens on a worker thread. No key is ever recorded: the hook
//! ignores every key except V.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    WAIT_TIMEOUT, WPARAM,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP};
use windows::Win32::System::Threading::{
    CreateMutexW, CreateProcessW, OpenProcess, WaitForSingleObject, DETACHED_PROCESS,
    PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, STARTUPINFOW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, FindWindowW, GetAncestor, GetCursorPos, GetForegroundWindow,
    GetGUIThreadInfo, GetMessageW, GetWindowThreadProcessId, LoadIconW, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    SetWindowsHookExW, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, GA_PARENT,
    GUITHREADINFO, HC_ACTION, IDI_APPLICATION, KBDLLHOOKSTRUCT, MB_ICONWARNING, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, TPM_RIGHTBUTTON, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WM_APP,
    WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW, WS_OVERLAPPED,
};

use super::rules::{takes_over, Focus, Modifiers, Offer};
use crate::explorer;

const WINDOW_CLASS: PCWSTR = w!("clipsave-agent");
const WM_TRAY: u32 = WM_APP + 1;
const ID_QUIT: usize = 1;
const VK_V: u32 = b'V' as u32;

/// Foreground windows (as isize) to paste into, sent from the hook.
static JOBS: OnceLock<Sender<isize>> = OnceLock::new();
/// Clipboard format ids registered by name, looked up once.
static FORMATS: OnceLock<Formats> = OnceLock::new();
/// Set while a V press we took over is held, so auto-repeat does not save
/// a burst of files and the matching key-up is swallowed too.
static V_TAKEN: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Formats {
    png: u32,
    /// Formats Explorer pastes itself.
    shell: [u32; 4],
}

/// Runs the agent until it is told to quit (tray menu, `clipsave agent stop`,
/// uninstall). Returns at once if another agent is already running.
pub fn run() -> Result<String, String> {
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\clipsave-agent"))
            .map_err(|e| format!("Cannot start the agent: {e}"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok("The clipsave agent is already running.".into());
        }

        FORMATS.get_or_init(|| Formats {
            png: RegisterClipboardFormatW(w!("PNG")),
            shell: [
                RegisterClipboardFormatW(w!("Shell IDList Array")),
                RegisterClipboardFormatW(w!("FileGroupDescriptorW")),
                RegisterClipboardFormatW(w!("FileGroupDescriptor")),
                RegisterClipboardFormatW(w!("FileContents")),
            ],
        });

        let (tx, rx) = channel::<isize>();
        let _ = JOBS.set(tx);
        std::thread::spawn(move || {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            for hwnd in rx {
                paste_into(HWND(hwnd as *mut _));
            }
        });

        let instance: HINSTANCE = GetModuleHandleW(None).map_err(|e| e.to_string())?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        // Hidden top-level window: receives tray clicks, WM_CLOSE from
        // `clipsave agent stop`, and TaskbarCreated after Explorer restarts.
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            WINDOW_CLASS,
            w!("clipsave"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(|e| format!("Cannot create the agent window: {e}"))?;
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        tray(hwnd, NIM_ADD);

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(instance), 0)
            .map_err(|e| format!("Cannot install the keyboard hook: {e}"))?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
    Ok(String::new())
}

/// Asks a running agent to quit and waits for it to exit, so its exe can be
/// replaced or deleted. Returns whether one was running.
pub fn stop() -> bool {
    unsafe {
        let Ok(hwnd) = FindWindowW(WINDOW_CLASS, PCWSTR::null()) else {
            return false;
        };
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid);
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        if let Ok(process) = process {
            if WaitForSingleObject(process, 5000) == WAIT_TIMEOUT {
                eprintln!("clipsave: the agent did not exit within 5 s");
            }
            let _ = CloseHandle(process);
        }
        true
    }
}

/// Starts `exe agent` in the background.
///
/// Uses CreateProcessW with handle inheritance off. `std::process::Command`
/// inherits every inheritable handle, including our own stdout pipe, and the
/// long-lived agent would then keep a caller of `clipsave install` waiting
/// for end-of-output forever.
pub fn start(exe: &Path) -> Result<(), String> {
    let mut cmdline: Vec<u16> = format!("\"{}\" agent", exe.display())
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            None,
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            DETACHED_PROCESS,
            None,
            None,
            &startup,
            &mut info,
        )
        .map_err(|e| format!("Cannot start the agent: {e}"))?;
        let _ = CloseHandle(info.hThread);
        let _ = CloseHandle(info.hProcess);
    }
    Ok(())
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let key = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if key.vkCode == VK_V {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    if V_TAKEN.load(Ordering::Relaxed) {
                        if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 {
                            return LRESULT(1); // auto-repeat of a press we took over
                        }
                        // Ctrl is up, so we missed V's key-up (e.g. Win+L
                        // mid-press): never swallow a plain `v`.
                        V_TAKEN.store(false, Ordering::Relaxed);
                    }
                    let fg = GetForegroundWindow();
                    if should_take_over(fg) {
                        if let Some(jobs) = JOBS.get() {
                            V_TAKEN.store(true, Ordering::Relaxed);
                            let _ = jobs.send(fg.0 as isize);
                            return LRESULT(1);
                        }
                    }
                }
                WM_KEYUP | WM_SYSKEYUP if V_TAKEN.swap(false, Ordering::Relaxed) => {
                    return LRESULT(1);
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Cheap checks only: window classes, key state, clipboard format presence.
unsafe fn should_take_over(fg: HWND) -> bool {
    let down = |vk: u16| GetAsyncKeyState(vk as i32) < 0;
    let keys = Modifiers {
        ctrl: down(VK_CONTROL.0),
        shift: down(VK_SHIFT.0),
        alt: down(VK_MENU.0),
        win: down(VK_LWIN.0) || down(VK_RWIN.0),
    };
    if !keys.ctrl || fg.is_invalid() {
        return false;
    }

    let mut info = GUITHREADINFO {
        cbSize: size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let thread = GetWindowThreadProcessId(fg, None);
    if GetGUIThreadInfo(thread, &mut info).is_err() || info.hwndFocus.is_invalid() {
        return false;
    }
    let top = explorer::class_name(fg);
    let focus = explorer::class_name(info.hwndFocus);
    let focus_parent = explorer::class_name(GetAncestor(info.hwndFocus, GA_PARENT));

    let Some(formats) = FORMATS.get() else {
        return false;
    };
    let has = |format: u32| format != 0 && IsClipboardFormatAvailable(format).is_ok();
    let offer = Offer {
        image: has(formats.png)
            || has(CF_DIBV5.0 as u32)
            || has(CF_DIB.0 as u32)
            || has(CF_BITMAP.0 as u32),
        shell_paste: has(CF_HDROP.0 as u32) || formats.shell.iter().any(|&f| has(f)),
    };

    takes_over(
        keys,
        Focus {
            top: &top,
            focus: &focus,
            focus_parent: &focus_parent,
        },
        offer,
    )
}

/// Worker thread: save into the folder the window shows, then select the file.
fn paste_into(window: HWND) {
    let Some(dir) = explorer::folder_of_window(window) else {
        // Home, This PC, Gallery...: no folder on disk to save into.
        unsafe {
            let _ = MessageBeep(MB_ICONWARNING);
        }
        return;
    };
    match crate::save_clipboard(&dir, None) {
        Ok(path) => explorer::reveal(&path),
        Err(e) => crate::notify::error(&e.message),
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if matches!(event, WM_RBUTTONUP | WM_LBUTTONUP | WM_CONTEXTMENU) {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND if wparam.0 & 0xFFFF == ID_QUIT => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            tray(hwnd, NIM_DELETE);
            PostQuitMessage(0);
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            tray(hwnd, NIM_ADD); // Explorer restarted: the icon is gone
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn tray(hwnd: HWND, action: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        ..Default::default()
    };
    let tip: Vec<u16> = "clipsave: Ctrl+V in Explorer saves clipboard images"
        .encode_utf16()
        .collect();
    data.szTip[..tip.len()].copy_from_slice(&tip);
    let _ = Shell_NotifyIconW(action, &data);
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let _ = AppendMenuW(
        menu,
        MF_STRING | MF_GRAYED,
        0,
        w!("Ctrl+V in Explorer saves clipboard images"),
    );
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit clipsave"));
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Required so the menu closes when the user clicks elsewhere.
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
}
