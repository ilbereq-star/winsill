// Debug builds keep a console to print to; release builds are a plain window program.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    win::run();
    #[cfg(not(windows))]
    eprintln!("winsill-strip draws a native Windows window; on this system, use Perch instead.");
}
