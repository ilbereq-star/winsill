//! What each cell of the strip looks like right now: its sprite, the tile under it and how faint it is.

use crate::layout::{Edge, TAB_PIXELS};
use crate::protocol::{Session, State};
use crate::sessions::{mood, needs_attention};
use crate::sprite::{Mood, Pixels, Rgb, SPRITE_SIZE, sprite_frame};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub rgb: Rgb,
    /// 0…1
    pub alpha: f32,
}

impl Paint {
    pub const ACCENT: Paint = Paint { rgb: Rgb::new(0xff, 0x6b, 0x1a), alpha: 1.0 };
    pub const DANGER: Paint = Paint { rgb: Rgb::new(0xff, 0x4d, 0x4d), alpha: 1.0 };
    /// A session that finished its turn and waits for the user's reply.
    pub const READY: Paint = Paint { rgb: Rgb::new(0x4c, 0xd9, 0x64), alpha: 1.0 };

    pub const fn white(alpha: f32) -> Paint {
        Paint { rgb: Rgb::new(255, 255, 255), alpha }
    }

    pub const fn with_alpha(self, alpha: f32) -> Paint {
        Paint { rgb: self.rgb, alpha }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub fill: Option<Paint>,
    pub border: Option<Paint>,
    /// In points: 1 is a pixel at 96 DPI.
    pub border_width: f32,
}

impl Default for Tile {
    fn default() -> Self {
        Tile { fill: None, border: None, border_width: 1.5 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub pixels: Pixels,
    pub tile: Tile,
    pub opacity: f32,
}

const CONTROL_COLOR: Rgb = Rgb::new(0x8b, 0x90, 0x9a);

fn hover_tile(hovered: bool) -> Tile {
    if hovered { Tile { fill: Some(Paint::white(0.12)), ..Tile::default() } } else { Tile::default() }
}

impl Appearance {
    /// The tile at the end of the strip that pins sessions: a plus in its middle.
    pub fn add_button(hovered: bool) -> Appearance {
        let mut pixels = Pixels::new(SPRITE_SIZE, SPRITE_SIZE);
        for x in 8..=15 {
            for y in 11..=12 {
                pixels.set(x, y, CONTROL_COLOR);
            }
        }
        for x in 11..=12 {
            for y in 8..=15 {
                pixels.set(x, y, CONTROL_COLOR);
            }
        }
        Appearance { pixels, tile: hover_tile(hovered), opacity: 1.0 }
    }

    /// The tab that collapses the strip: an arrow toward the edge while open, back out while collapsed.
    /// Collapsed, it carries the mark of its most urgent session.
    pub fn tab(sessions: &[&Session], collapsed: bool, edge: Edge, hovered: bool, frame: i64, now: i64) -> Appearance {
        const CHEVRON: [&str; 7] = ["...X", "..XX", ".XX.", "XX..", ".XX.", "..XX", "...X"];
        let (width, height) = (TAB_PIXELS.0 as usize, TAB_PIXELS.1 as usize);
        let left = (width - 4) / 2;
        let top = (height - CHEVRON.len()) / 2;
        let points_right = (edge == Edge::Right) != collapsed;
        let mut pixels = Pixels::new(width, height);
        for (y, row) in CHEVRON.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'X' {
                    let x = if points_right { 3 - x } else { x };
                    pixels.set((left + x) as i32, (top + y) as i32, CONTROL_COLOR);
                }
            }
        }

        let mut tile = hover_tile(hovered);
        let most_urgent = sessions
            .iter()
            .filter_map(|s| urgency(s, now).map(|rank| (rank, *s)))
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, s)| s);
        if collapsed && let Some(urgent) = most_urgent {
            tile = marked(tile, urgent, frame, now);
            if needs_attention(urgent) {
                pixels = outlined(&pixels);
            }
        }
        Appearance { pixels, tile, opacity: 1.0 }
    }

    pub fn session(session: &Session, focused: bool, hovered: bool, frame: i64, now: i64) -> Appearance {
        let mut tile = Tile::default();
        if hovered {
            tile.fill = Some(Paint::white(0.12));
        }
        if focused {
            tile.fill = Some(Paint::white(if hovered { 0.3 } else { 0.22 }));
            tile.border = Some(Paint::white(0.75));
        }
        tile = marked(tile, session, frame, now);

        let color = Rgb::from_hex(&session.icon.color).unwrap_or(Rgb::new(0xd9, 0x77, 0x57));
        let mut pixels = sprite_frame(&session.icon.character, color, mood(session, now), frame);
        // An accent tile may share the sprite's color.
        if needs_attention(session) {
            pixels = outlined(&pixels);
        }
        if !session.live {
            pixels = grayscaled(&pixels);
        }
        Appearance { pixels, tile, opacity: if session.live { 1.0 } else { 0.45 } }
    }
}

/// Attention outranks focus and hover.
fn marked(tile: Tile, session: &Session, frame: i64, now: i64) -> Tile {
    let mut tile = tile;
    match session.state {
        State::NeedsPermission => {
            let fill = if frame.rem_euclid(4) < 2 { Paint::ACCENT } else { Paint::ACCENT.with_alpha(0.25) };
            tile = Tile { fill: Some(fill), border: Some(Paint::ACCENT), border_width: 2.0 };
        }
        State::NeedsInput => {
            let alpha = if frame.rem_euclid(8) < 4 { 0.35 } else { 0.2 };
            tile = Tile { fill: Some(Paint::ACCENT.with_alpha(alpha)), border: Some(Paint::ACCENT), border_width: 2.0 };
        }
        State::Done if mood(session, now) == Mood::Finished => tile.border = Some(Paint::READY),
        State::Error => tile.border = Some(Paint::DANGER),
        _ => {}
    }
    tile
}

/// Lower is more urgent; None carries no mark.
fn urgency(session: &Session, now: i64) -> Option<u8> {
    match session.state {
        State::NeedsPermission => Some(0),
        State::NeedsInput => Some(1),
        State::Error => Some(2),
        State::Done if mood(session, now) == Mood::Finished => Some(3),
        _ => None,
    }
}

fn outlined(pixels: &Pixels) -> Pixels {
    let black = Rgb::new(0, 0, 0);
    let mut result = pixels.clone();
    for y in 0..pixels.height as i32 {
        for x in 0..pixels.width as i32 {
            if pixels.is_filled(x, y) {
                continue;
            }
            let touches_sprite =
                pixels.is_filled(x - 1, y) || pixels.is_filled(x + 1, y) || pixels.is_filled(x, y - 1) || pixels.is_filled(x, y + 1);
            if touches_sprite {
                result.set(x, y, black);
            }
        }
    }
    result
}

fn grayscaled(pixels: &Pixels) -> Pixels {
    pixels.map(|c| {
        let luma = (0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b)).round() as u8;
        Rgb::new(luma, luma, luma)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::session;

    #[test]
    fn a_session_waiting_for_permission_blinks_orange() {
        let s = session("a", "api", State::NeedsPermission);
        let lit = Appearance::session(&s, false, false, 0, 0);
        let dim = Appearance::session(&s, false, false, 2, 0);
        assert_eq!(lit.tile.fill, Some(Paint::ACCENT));
        assert_eq!(dim.tile.fill, Some(Paint::ACCENT.with_alpha(0.25)));
        assert_eq!((lit.tile.border, lit.tile.border_width), (Some(Paint::ACCENT), 2.0));
    }

    #[test]
    fn attention_outranks_focus() {
        let s = session("a", "api", State::NeedsInput);
        let focused = Appearance::session(&s, true, true, 0, 0);
        assert_eq!(focused.tile.border, Some(Paint::ACCENT));
        let calm = session("a", "api", State::Working);
        assert_eq!(Appearance::session(&calm, true, false, 0, 0).tile.border, Some(Paint::white(0.75)));
        assert_eq!(Appearance::session(&calm, false, true, 0, 0).tile.fill, Some(Paint::white(0.12)));
    }

    #[test]
    fn a_finished_session_gets_a_green_ring_and_an_error_a_red_one() {
        let done = session("a", "api", State::Done);
        assert_eq!(Appearance::session(&done, false, false, 0, 0).tile.border, Some(Paint::READY));
        let error = session("a", "api", State::Error);
        assert_eq!(Appearance::session(&error, false, false, 0, 0).tile.border, Some(Paint::DANGER));
    }

    #[test]
    fn a_session_that_is_not_running_is_gray_and_faint() {
        let mut s = session("a", "api", State::Idle);
        s.live = false;
        let a = Appearance::session(&s, false, false, 1, 0);
        assert_eq!(a.opacity, 0.45);
        assert!(a.pixels.trimmed().width > 0);
        let all_gray = (0..24).all(|y| (0..24).all(|x| a.pixels.get(x, y).is_none_or(|c| c.r == c.g && c.g == c.b)));
        assert!(all_gray);
    }

    #[test]
    fn the_outline_wraps_a_creature_that_needs_attention() {
        let plain = Appearance::session(&session("a", "api", State::Working), false, false, 0, 0);
        let needy = Appearance::session(&session("a", "api", State::NeedsPermission), false, false, 0, 0);
        assert!(needy.pixels.filled() > 0 && plain.pixels.filled() > 0);
        let outline_only = |a: &Appearance| (0..24).flat_map(|y| (0..24).map(move |x| (x, y))).filter(|&(x, y)| a.pixels.get(x, y) == Some(Rgb::new(0, 0, 0))).count();
        assert_eq!(outline_only(&plain), 0);
        assert!(outline_only(&needy) > 20);
    }

    #[test]
    fn the_add_button_is_a_plus() {
        let a = Appearance::add_button(false);
        assert_eq!(a.pixels.filled(), 8 * 2 + 2 * 8 - 2 * 2);
        assert_eq!(a.pixels.get(11, 8), Some(CONTROL_COLOR));
        assert_eq!(a.pixels.get(8, 11), Some(CONTROL_COLOR));
        assert_eq!(a.pixels.get(8, 8), None);
        assert_eq!(Appearance::add_button(true).tile.fill, Some(Paint::white(0.12)));
    }

    #[test]
    fn the_tab_points_at_the_edge_while_open_and_away_while_collapsed() {
        let open_right = Appearance::tab(&[], false, Edge::Right, false, 0, 0);
        let collapsed_right = Appearance::tab(&[], true, Edge::Right, false, 0, 0);
        let open_left = Appearance::tab(&[], false, Edge::Left, false, 0, 0);
        assert_ne!(open_right.pixels, collapsed_right.pixels);
        assert_eq!(collapsed_right.pixels, open_left.pixels, "same arrow, other edge");
        assert_eq!(open_right.pixels.filled(), 12);
    }

    #[test]
    fn a_collapsed_tab_carries_the_most_urgent_mark() {
        let permission = session("a", "api", State::NeedsPermission);
        let error = session("b", "web", State::Error);
        let tab = Appearance::tab(&[&error, &permission], true, Edge::Right, false, 0, 0);
        assert_eq!(tab.tile.fill, Some(Paint::ACCENT));
        let open = Appearance::tab(&[&error, &permission], false, Edge::Right, false, 0, 0);
        assert_eq!(open.tile, Tile::default(), "open, the tab carries nothing");
        let only_error = Appearance::tab(&[&error], true, Edge::Right, false, 0, 0);
        assert_eq!(only_error.tile.border, Some(Paint::DANGER));
    }
}
