//! Which monitor the strip sits on. Windows lists them; this decides which one to use.

use crate::layout::Rect;

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// Windows' name for it, like `\\.\DISPLAY2`: it stays the same across restarts, unlike its position.
    pub name: String,
    /// The monitor minus the taskbar, in virtual-screen pixels.
    pub work: Rect,
    /// The monitor's size, for telling them apart in a menu.
    pub size: (i32, i32),
    /// DPI factor: 1.0 at 96 DPI.
    pub ui: f32,
    pub primary: bool,
}

impl Screen {
    /// `1: 1366×768`, from `\\.\DISPLAY1`.
    pub fn label(&self) -> String {
        let number: String = self.name.chars().filter(char::is_ascii_digit).collect();
        let (w, h) = self.size;
        if number.is_empty() { format!("{w}×{h}") } else { format!("{number}: {w}×{h}") }
    }
}

/// The monitor called `wanted`; the primary one when nothing is wanted or the wanted one is gone
/// (unplugged, say), so the strip never ends up off every screen.
pub fn pick<'a>(screens: &'a [Screen], wanted: Option<&str>) -> Option<&'a Screen> {
    wanted
        .and_then(|name| screens.iter().find(|s| s.name == name))
        .or_else(|| screens.iter().find(|s| s.primary))
        .or_else(|| screens.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(name: &str, primary: bool) -> Screen {
        Screen { name: name.into(), work: Rect::new(0, 0, 1920, 1040), size: (1920, 1080), ui: 1.0, primary }
    }

    #[test]
    fn the_wanted_monitor_wins_when_it_is_there() {
        let screens = [screen(r"\\.\DISPLAY1", true), screen(r"\\.\DISPLAY2", false)];
        assert_eq!(pick(&screens, Some(r"\\.\DISPLAY2")).unwrap().name, r"\\.\DISPLAY2");
    }

    #[test]
    fn otherwise_the_primary_one() {
        let screens = [screen(r"\\.\DISPLAY2", false), screen(r"\\.\DISPLAY1", true)];
        assert_eq!(pick(&screens, None).unwrap().name, r"\\.\DISPLAY1");
        assert_eq!(pick(&screens, Some(r"\\.\DISPLAY9")).unwrap().name, r"\\.\DISPLAY1");
    }

    #[test]
    fn with_no_primary_flag_the_first_one_and_with_none_nothing() {
        assert_eq!(pick(&[screen("a", false)], None).unwrap().name, "a");
        assert!(pick(&[], Some("a")).is_none());
    }

    #[test]
    fn labels_tell_monitors_apart() {
        assert_eq!(screen(r"\\.\DISPLAY2", false).label(), "2: 1920×1080");
        assert_eq!(screen("laptop", false).label(), "1920×1080");
    }
}
