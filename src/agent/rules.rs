//! When the agent takes over Ctrl+V. Pure logic, unit-tested on every OS:
//! a mistake here would break Ctrl+V system-wide, so anything unexpected
//! passes the key through untouched.

/// Modifier keys held when V went down.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

/// Window classes at the moment of the key press.
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus<'a> {
    /// Class of the foreground (top-level) window.
    pub top: &'a str,
    /// Class of the window with keyboard focus inside it.
    pub focus: &'a str,
    /// Class of that window's parent.
    pub focus_parent: &'a str,
}

/// What the clipboard offers (format availability only, no data read).
#[derive(Clone, Copy, Debug, Default)]
pub struct Offer {
    /// `PNG`, `CF_DIBV5`, `CF_DIB` or `CF_BITMAP`.
    pub image: bool,
    /// Anything Explorer can paste itself: `CF_HDROP`, `Shell IDList Array`,
    /// `FileGroupDescriptor(W)` / `FileContents` (Outlook attachments, zips).
    pub shell_paste: bool,
}

/// Swallow Ctrl+V and save the image only when Explorer itself would paste
/// nothing: plain Ctrl+V, focus on the item view of an Explorer window or the
/// desktop (not the rename box, address bar, search box or tree), and an
/// image without any file content on the clipboard.
pub fn takes_over(keys: Modifiers, focus: Focus, offer: Offer) -> bool {
    let plain_ctrl = keys.ctrl && !keys.shift && !keys.alt && !keys.win;
    let explorer = matches!(focus.top, "CabinetWClass" | "ExploreWClass");
    let desktop = matches!(focus.top, "Progman" | "WorkerW");
    // The item view sits directly under SHELLDLL_DefView: DirectUIHWND in
    // Explorer windows, SysListView32 on the desktop.
    let item_view = focus.focus_parent == "SHELLDLL_DefView"
        && matches!(focus.focus, "DirectUIHWND" | "SysListView32");

    plain_ctrl && (explorer || desktop) && item_view && offer.image && !offer.shell_paste
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        shift: false,
        alt: false,
        win: false,
    };
    const FILE_LIST: Focus = Focus {
        top: "CabinetWClass",
        focus: "DirectUIHWND",
        focus_parent: "SHELLDLL_DefView",
    };
    const DESKTOP: Focus = Focus {
        top: "Progman",
        focus: "SysListView32",
        focus_parent: "SHELLDLL_DefView",
    };
    const IMAGE: Offer = Offer {
        image: true,
        shell_paste: false,
    };

    #[test]
    fn image_in_file_list_or_desktop() {
        assert!(takes_over(CTRL, FILE_LIST, IMAGE));
        assert!(takes_over(CTRL, DESKTOP, IMAGE));
        assert!(takes_over(
            CTRL,
            Focus {
                top: "WorkerW",
                ..DESKTOP
            },
            IMAGE
        ));
    }

    #[test]
    fn explorer_keeps_anything_it_can_paste() {
        // copied files, Outlook attachments, zip entries: Explorer's own paste
        assert!(!takes_over(
            CTRL,
            FILE_LIST,
            Offer {
                image: true,
                shell_paste: true
            }
        ));
        assert!(!takes_over(
            CTRL,
            FILE_LIST,
            Offer {
                image: false,
                shell_paste: true
            }
        ));
        // text only: nothing for us
        assert!(!takes_over(CTRL, FILE_LIST, Offer::default()));
    }

    #[test]
    fn only_plain_ctrl_v() {
        for keys in [
            Modifiers {
                shift: true,
                ..CTRL
            },
            Modifiers { alt: true, ..CTRL },
            Modifiers { win: true, ..CTRL },
            Modifiers::default(),
        ] {
            assert!(!takes_over(keys, FILE_LIST, IMAGE), "{keys:?}");
        }
    }

    #[test]
    fn not_in_text_boxes_or_other_panes() {
        let cases = [
            // rename box
            Focus {
                focus: "Edit",
                focus_parent: "DirectUIHWND",
                ..FILE_LIST
            },
            Focus {
                focus: "Edit",
                focus_parent: "SHELLDLL_DefView",
                ..FILE_LIST
            },
            // address bar / search box (XAML islands on Windows 11)
            Focus {
                focus: "Microsoft.UI.Content.DesktopChildSiteBridge",
                focus_parent: "DesktopWindowXamlSource",
                ..FILE_LIST
            },
            // navigation pane
            Focus {
                focus: "SysTreeView32",
                focus_parent: "NamespaceTreeControl",
                ..FILE_LIST
            },
            // unknown
            Focus {
                focus: "",
                focus_parent: "",
                ..FILE_LIST
            },
        ];
        for focus in cases {
            assert!(!takes_over(CTRL, focus, IMAGE), "{focus:?}");
        }
    }

    #[test]
    fn not_in_other_apps() {
        // e.g. a file Open/Save dialog hosts the same item view
        for top in ["#32770", "Chrome_WidgetWin_1", "Notepad", ""] {
            assert!(
                !takes_over(CTRL, Focus { top, ..FILE_LIST }, IMAGE),
                "{top}"
            );
        }
    }
}
