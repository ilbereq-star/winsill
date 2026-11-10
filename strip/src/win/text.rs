//! Text for the hover label. GDI draws the glyphs into a scratch bitmap as white on black, and what
//! comes out is a coverage mask the rasterizer lays over the label in any color. That way the text
//! gets the label's own alpha, which GDI itself can't give.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_NATURAL_QUALITY, FONT_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CALCRECT, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DeleteDC,
    DeleteObject, DrawTextW, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HFONT, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows::core::PCWSTR;

use super::wide;

pub struct Mask {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

/// The label's two fonts at one DPI.
pub struct Fonts {
    /// A session's name.
    pub name: HFONT,
    /// What it is doing.
    pub status: HFONT,
}

impl Fonts {
    pub fn new(ui: f32) -> Fonts {
        let make = |pixels: f32, weight: i32, face: &str| unsafe {
            let face = wide(face);
            CreateFontW(
                -(pixels * ui).round() as i32,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                FONT_QUALITY(CLEARTYPE_NATURAL_QUALITY as u8), // its three subpixels are averaged into one coverage
                FF_DONTCARE.0 as u32,
                PCWSTR(face.as_ptr()),
            )
        };
        Fonts { name: make(12.0, FW_SEMIBOLD.0 as i32, "Segoe UI"), status: make(11.0, FW_NORMAL.0 as i32, "Consolas") }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.name.into());
            let _ = DeleteObject(self.status.into());
        }
    }
}

fn thinned(coverage: u8) -> u8 {
    ((f32::from(coverage) / 255.0).powf(GAMMA) * 255.0).round() as u8
}

const GAMMA: f32 = 1.3;

/// One line of `text` in `font`, cut with an ellipsis past `max_width` pixels.
pub fn mask(font: HFONT, text: &str, max_width: i32) -> Mask {
    unsafe {
        let dc = CreateCompatibleDC(None);
        let previous_font = SelectObject(dc, font.into());
        let mut chars = wide(text);
        chars.pop(); // DrawTextW takes the length, not a terminator
        let format = DRAW_TEXT_FORMAT(DT_SINGLELINE.0 | DT_NOPREFIX.0 | DT_LEFT.0);

        let mut measured = RECT::default();
        DrawTextW(dc, &mut chars, &mut measured, DRAW_TEXT_FORMAT(format.0 | DT_CALCRECT.0));
        let width = measured.right.min(max_width).max(1);
        let height = measured.bottom.max(1);

        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return Mask { width: 1, height: 1, data: vec![0] };
        };
        let previous_bitmap = SelectObject(dc, bitmap.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        let mut area = RECT { left: 0, top: 0, right: width, bottom: height };
        DrawTextW(dc, &mut chars, &mut area, DRAW_TEXT_FORMAT(format.0 | DT_END_ELLIPSIS.0));

        // White on black in grayscale: any channel is the coverage. Light text on a dark ground looks
        // heavier than the same coverage the other way round, so the curve thins the partial pixels.
        let pixels = std::slice::from_raw_parts(bits as *const u8, width as usize * height as usize * 4);
        let data = pixels.chunks_exact(4).map(|p| thinned(((u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3) as u8)).collect();

        SelectObject(dc, previous_bitmap);
        SelectObject(dc, previous_font);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Mask { width, height, data }
    }
}
