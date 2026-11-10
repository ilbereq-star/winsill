//! Popup menus. What a menu holds is plain data (`Popup`); showing it runs Windows' own modal loop
//! and says which entry was picked, so nothing here needs the app's state.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MENU_ITEM_FLAGS, PostMessageW, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TPM_VCENTERALIGN,
    TRACK_POPUP_MENU_FLAGS, TrackPopupMenu, WM_NULL,
};
use windows::core::PCWSTR;

use super::wide;

pub enum Item {
    /// Greyed text that names what the menu is about.
    Header(String),
    /// `command` is what `show` returns when it is picked.
    Entry { text: String, command: usize, checked: bool },
    Separator,
    Submenu(String, Vec<Item>),
}

pub struct Popup {
    pub items: Vec<Item>,
    /// Screen coordinates the menu opens beside.
    pub at: (i32, i32),
    /// Open toward the left of `at`, for a strip on the right edge.
    pub leftward: bool,
}

/// Shows `popup` and returns the `command` of the entry picked, or None if it was dismissed.
pub fn show(owner: HWND, popup: &Popup) -> Option<usize> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        fill(menu, &popup.items);
        // The menu closes when the user clicks elsewhere only if its owner is in front.
        let _ = SetForegroundWindow(owner);
        let align = if popup.leftward { TPM_RIGHTALIGN } else { TPM_LEFTALIGN };
        let flags = TRACK_POPUP_MENU_FLAGS(TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0 | TPM_VCENTERALIGN.0 | align.0);
        let picked = TrackPopupMenu(menu, flags, popup.at.0, popup.at.1, None, owner, None).0;
        let _ = PostMessageW(Some(owner), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu); // also destroys its submenus
        usize::try_from(picked).ok().filter(|&id| id > 0).map(|id| id - 1)
    }
}

// Menu ids are `command + 1`, because 0 means nothing was picked.
unsafe fn fill(menu: HMENU, items: &[Item]) {
    unsafe {
        for item in items {
            match item {
                Item::Header(text) => {
                    let text = wide(text);
                    let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0 | MF_DISABLED.0 | MF_GRAYED.0), 0, PCWSTR(text.as_ptr()));
                }
                Item::Entry { text, command, checked } => {
                    let text = wide(text);
                    let flags = MF_STRING.0 | if *checked { MF_CHECKED.0 } else { 0 };
                    let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(flags), command + 1, PCWSTR(text.as_ptr()));
                }
                Item::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Item::Submenu(text, children) => {
                    let Ok(submenu) = CreatePopupMenu() else { continue };
                    fill(submenu, children);
                    let text = wide(text);
                    let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0 | MF_POPUP.0), submenu.0 as usize, PCWSTR(text.as_ptr()));
                }
            }
        }
    }
}
