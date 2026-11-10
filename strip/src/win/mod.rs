//! The Windows side: a layered window per overlay, the socket to the agent, menus and the tray icon.

mod app;
mod link;
mod menu;
mod text;
mod tray;
mod window;

pub fn run() {
    app::run();
}

/// A NUL-terminated UTF-16 copy of `text`, for the wide Win32 calls.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
