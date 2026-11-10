//! The notification-area icon: the default creature, with the count of sessions that wait for you in its tooltip.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, GetDC, ReleaseDC,
};
use windows::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON};
use winsill_strip::sprite::{Mood, Rgb, sprite_frame};

const ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    callback: u32,
    icon: HICON,
    tip: String,
}

impl Tray {
    /// Adds the icon; `callback` is the window message its clicks arrive as.
    pub fn add(hwnd: HWND, callback: u32) -> Tray {
        let mut tray = Tray { hwnd, callback, icon: build_icon(), tip: "WinSill".into() };
        tray.notify(NIM_ADD);
        tray
    }

    /// Explorer forgets its icons when it restarts; this puts ours back.
    pub fn re_add(&mut self) {
        self.notify(NIM_ADD);
    }

    pub fn set_tip(&mut self, tip: &str) {
        if self.tip != tip {
            self.tip = tip.to_owned();
            self.notify(NIM_MODIFY);
        }
    }

    fn notify(&mut self, message: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE) {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: self.callback,
            hIcon: self.icon,
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip(self.tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        unsafe {
            let _ = Shell_NotifyIconW(message, &data);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: ID, ..Default::default() };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            let _ = DestroyIcon(self.icon);
        }
    }
}

/// The default creature, as big as the notification area wants its icons, in the color of a fresh session.
fn build_icon() -> HICON {
    unsafe {
        let side = GetSystemMetrics(SM_CXSMICON).max(16);
        let creature = sprite_frame("default", Rgb::new(0xd9, 0x77, 0x57), Mood::Idle, 1).trimmed();
        let scale = (side / creature.width.max(1) as i32).max(1);
        let left = (side - creature.width as i32 * scale) / 2;
        let top = (side - creature.height as i32 * scale) / 2;

        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let screen = GetDC(None);
        let mut bits: *mut c_void = null_mut();
        let color = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0).expect("cannot create the tray icon");
        ReleaseDC(None, screen);
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (side * side * 4) as usize);
        for y in 0..side {
            for x in 0..side {
                let (sx, sy) = ((x - left).div_euclid(scale), (y - top).div_euclid(scale));
                if x < left || y < top {
                    continue;
                }
                if let Some(c) = creature.get(sx, sy) {
                    let i = ((y * side + x) * 4) as usize;
                    pixels[i..i + 4].copy_from_slice(&[c.b, c.g, c.r, 255]);
                }
            }
        }
        let mask = CreateBitmap(side, side, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color })
            .expect("cannot create the tray icon");
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}
