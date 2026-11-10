//! A layered window that shows a premultiplied-BGRA canvas, with per-pixel alpha.
//! Pixels with alpha 0 let the mouse through, so the window can be bigger than what it draws.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{copy_nonoverlapping, null_mut};

use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, SetWindowPos, ULW_ALPHA, ShowWindow,
    UpdateLayeredWindow,
};
use winsill_strip::raster::Canvas;

pub struct Surface {
    pub hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    size: (i32, i32),
    visible: bool,
}

impl Surface {
    pub fn new(hwnd: HWND) -> Surface {
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            Surface {
                hwnd,
                dc,
                bitmap: HBITMAP::default(),
                previous: HGDIOBJ::default(),
                bits: null_mut(),
                size: (0, 0),
                visible: false,
            }
        }
    }

    fn resize(&mut self, width: i32, height: i32) {
        unsafe {
            self.release_bitmap();
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top row first
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(Some(self.dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .expect("cannot create the strip's bitmap");
            self.previous = SelectObject(self.dc, bitmap.into());
            self.bitmap = bitmap;
            self.bits = bits.cast();
            self.size = (width, height);
        }
    }

    unsafe fn release_bitmap(&mut self) {
        unsafe {
            if !self.bitmap.is_invalid() {
                SelectObject(self.dc, self.previous);
                let _ = DeleteObject(self.bitmap.into());
                self.bitmap = HBITMAP::default();
                self.bits = null_mut();
            }
        }
    }

    /// Draws `canvas` with its top-left corner at screen position (`x`, `y`), showing the window if it was hidden.
    pub fn present(&mut self, canvas: &Canvas, x: i32, y: i32) {
        if canvas.width <= 0 || canvas.height <= 0 {
            return self.hide();
        }
        if self.size != (canvas.width, canvas.height) || self.bits.is_null() {
            self.resize(canvas.width, canvas.height);
        }
        unsafe {
            copy_nonoverlapping(canvas.bgra.as_ptr(), self.bits, canvas.bgra.len());
            let position = POINT { x, y };
            let size = SIZE { cx: canvas.width, cy: canvas.height };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(self.hwnd, None, Some(&position), Some(&size), Some(self.dc), Some(&source), COLORREF(0), Some(&blend), ULW_ALPHA);
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    /// Another topmost window (the taskbar, a full-screen app) can end up above ours; this puts us back on top.
    pub fn raise(&self) {
        if self.visible {
            unsafe {
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.release_bitmap();
            let _ = DeleteDC(self.dc);
        }
    }
}
