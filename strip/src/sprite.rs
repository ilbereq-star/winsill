//! The creatures. Grown from claude-panel's sprite renderer and the Swift port in
//! `Sources/PerchCore/Sprites`; the templates are the same `characters.json` the other faces read.

use std::sync::OnceLock;

use serde::Deserialize;

/// A 16×16 character plus room above and to the right for overlays.
pub const SPRITE_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// "#rrggbb"
    pub fn from_hex(hex: &str) -> Option<Rgb> {
        let digits = hex.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(digits, 16).ok()?;
        Some(Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// Mixed toward black (amount < 0) or white (amount > 0); |amount| is 0…1.
    pub fn shaded(self, amount: f64) -> Rgb {
        let target = if amount < 0.0 { 0.0 } else { 255.0 };
        let t = amount.abs();
        let mix = |v: u8| (f64::from(v) + (target - f64::from(v)) * t).round() as u8;
        Rgb::new(mix(self.r), mix(self.g), mix(self.b))
    }
}

/// A grid of pixels, each filled with a color or empty.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    cells: Vec<Option<Rgb>>,
}

impl Pixels {
    pub fn new(width: usize, height: usize) -> Pixels {
        Pixels { width, height, cells: vec![None; width * height] }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Rgb> {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return None;
        }
        self.cells[y as usize * self.width + x as usize]
    }

    /// Ignores anything outside the grid.
    pub fn set(&mut self, x: i32, y: i32, color: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.cells[y as usize * self.width + x as usize] = Some(color);
        }
    }

    pub fn is_filled(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_some()
    }

    pub fn map(&self, f: impl Fn(Rgb) -> Rgb) -> Pixels {
        Pixels { width: self.width, height: self.height, cells: self.cells.iter().map(|c| c.map(&f)).collect() }
    }

    pub fn filled(&self) -> usize {
        self.cells.iter().flatten().count()
    }

    /// The smallest grid that holds every filled pixel; empty when there is none.
    pub fn trimmed(&self) -> Pixels {
        let mut bounds: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cells[y * self.width + x].is_some() {
                    bounds = Some(match bounds {
                        None => (x, y, x, y),
                        Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                    });
                }
            }
        }
        let Some((left, top, right, bottom)) = bounds else { return Pixels::new(0, 0) };
        let mut out = Pixels::new(right - left + 1, bottom - top + 1);
        for y in top..=bottom {
            for x in left..=right {
                out.cells[(y - top) * out.width + (x - left)] = self.cells[y * self.width + x];
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Working,
    Alert,
    Asking,
    Compacting,
    Error,
    Sleeping,
    /// Done with its turn and waiting for the user's reply.
    Finished,
}

/// Read from characters.json, a format shared with claude-panel's Pi panel.
#[derive(Debug, Deserialize)]
pub struct CharacterTemplate {
    pub id: String,
    pub label: String,
    /// 16 rows of 16 cells: B body, D darker, L lighter, F face plate, P pink, "." empty.
    pub body: Vec<String>,
    /// Rows that replace the body from `row` down on alternate frames while working.
    pub walk: Option<Walk>,
    /// Top-left corners of the two 2×2 eyes.
    pub eyes: Eyes,
    #[serde(rename = "eyeColor")]
    pub eye_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Walk {
    pub row: usize,
    pub rows: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Eyes {
    pub y: i32,
    pub left: i32,
    pub right: i32,
}

/// The default comes first. The JSON ships inside the program, so failing to read it is a build bug.
pub fn characters() -> &'static [CharacterTemplate] {
    static ALL: OnceLock<Vec<CharacterTemplate>> = OnceLock::new();
    ALL.get_or_init(|| {
        let all: Vec<CharacterTemplate> =
            serde_json::from_str(include_str!("../../Sources/PerchCore/Resources/characters.json"))
                .expect("characters.json is malformed");
        assert!(!all.is_empty(), "characters.json is empty");
        all
    })
}

pub fn character(id: &str) -> &'static CharacterTemplate {
    characters().iter().find(|c| c.id == id).unwrap_or(&characters()[0])
}

const BODY_X: i32 = 4;
const BODY_Y: i32 = 6;

mod palette {
    use super::Rgb;
    pub const BANG: Rgb = Rgb::new(0xff, 0xd2, 0x3f);
    pub const QUESTION: Rgb = Rgb::new(0xff, 0xff, 0xff);
    pub const CROSS: Rgb = Rgb::new(0xff, 0x4d, 0x4d);
    pub const Z: Rgb = Rgb::new(0xc8, 0xd0, 0xdc);
    pub const DOT_ON: Rgb = Rgb::new(0xff, 0xff, 0xff);
    pub const DOT_OFF: Rgb = Rgb::new(0x4a, 0x4f, 0x5a);
    pub const FACE: Rgb = Rgb::new(0x16, 0x17, 0x1c);
    pub const PINK: Rgb = Rgb::new(0xff, 0x8f, 0xb1);
    pub const CHECK: Rgb = Rgb::new(0x4c, 0xd9, 0x64);
    pub const EYE: Rgb = Rgb::new(0x11, 0x12, 0x16);
}

#[derive(Clone, Copy)]
enum Glyph {
    Bang,
    Question,
    Cross,
    Z,
    Dot,
    Check,
}

impl Glyph {
    fn rows(self) -> &'static [&'static str] {
        match self {
            Glyph::Bang => &["X", "X", "X", ".", "X"],
            Glyph::Question => &["XXX", "..X", ".XX", "...", ".X."],
            Glyph::Cross => &["X.X", ".X.", "X.X"],
            Glyph::Z => &["XXXX", "..X.", ".X..", "XXXX"],
            Glyph::Dot => &["X"],
            Glyph::Check => &[".....X", "....XX", "X..XX.", "XXXX..", ".XX..."],
        }
    }
}

const Z_SPOTS: [(i32, i32); 3] = [(15, 5), (18, 2), (20, 0)];

#[derive(Clone, Copy, PartialEq)]
enum EyeStyle {
    Open,
    Closed,
    Squint,
}

struct Overlay {
    glyph: Glyph,
    x: i32,
    y: i32,
    color: Rgb,
}

struct Pose {
    dx: i32,
    dy: i32,
    eyes: EyeStyle,
    walk: bool,
    overlays: Vec<Overlay>,
}

impl Pose {
    fn new(mood: Mood, frame: i64) -> Pose {
        // The value this frame lands on, cycling through `values`.
        fn cycle<T: Copy>(frame: i64, values: &[T]) -> T {
            values[frame.rem_euclid(values.len() as i64) as usize]
        }
        let blink = if frame.rem_euclid(16) == 0 { EyeStyle::Closed } else { EyeStyle::Open };
        let dot = |i: i32, y: i32, color: Rgb| Overlay { glyph: Glyph::Dot, x: 17 + i * 2, y, color };
        let mut pose = Pose { dx: 0, dy: 0, eyes: EyeStyle::Open, walk: false, overlays: vec![] };
        match mood {
            Mood::Idle => pose.eyes = blink,
            Mood::Working => {
                pose.dy = cycle(frame, &[0, -1, -2, -1]);
                pose.walk = cycle(frame, &[false, true]);
                let raised = cycle(frame, &[0, 1, 2]);
                pose.overlays = (0..3).map(|i| dot(i, if i == raised { 3 } else { 4 }, palette::DOT_ON)).collect();
            }
            Mood::Alert => {
                pose.dy = cycle(frame, &[0, -2, -3, -2]);
                if frame.rem_euclid(2) == 0 {
                    pose.overlays = vec![Overlay { glyph: Glyph::Bang, x: 21, y: 0, color: palette::BANG }];
                }
            }
            Mood::Asking => {
                pose.dy = cycle(frame, &[0, 0, -1, -1]);
                pose.overlays = vec![Overlay { glyph: Glyph::Question, x: 19, y: 0, color: palette::QUESTION }];
            }
            Mood::Compacting => {
                pose.dy = cycle(frame, &[0, 1]);
                pose.eyes = EyeStyle::Squint;
                let lit = cycle(frame, &[0, 1, 2]);
                pose.overlays = (0..3).map(|i| dot(i, 3, if i == lit { palette::DOT_ON } else { palette::DOT_OFF })).collect();
            }
            Mood::Error => {
                pose.dx = cycle(frame, &[-1, 1, -1, 0, 0, 0, 0, 0]);
                pose.eyes = EyeStyle::Squint;
                pose.overlays = vec![Overlay { glyph: Glyph::Cross, x: 19, y: 1, color: palette::CROSS }];
            }
            Mood::Finished => {
                pose.eyes = blink;
                pose.overlays = vec![Overlay { glyph: Glyph::Check, x: 17, y: 0, color: palette::CHECK }];
            }
            Mood::Sleeping => {
                let count = cycle(frame, &[1, 1, 2, 2, 3, 3, 0, 0]);
                pose.eyes = EyeStyle::Closed;
                pose.overlays = Z_SPOTS.iter().take(count).map(|&(x, y)| Overlay { glyph: Glyph::Z, x, y, color: palette::Z }).collect();
            }
        }
        pose
    }
}

fn eye_cells(style: EyeStyle, mirrored: bool) -> &'static [(i32, i32)] {
    match style {
        EyeStyle::Open => &[(0, 0), (1, 0), (0, 1), (1, 1)],
        EyeStyle::Closed => &[(0, 1), (1, 1)],
        EyeStyle::Squint if mirrored => &[(1, 0), (0, 1)],
        EyeStyle::Squint => &[(0, 0), (1, 1)],
    }
}

/// One frame of a creature: its color, what it is doing, and where in the animation it is.
pub fn sprite_frame(character_id: &str, color: Rgb, mood: Mood, frame: i64) -> Pixels {
    let template = character(character_id);
    let pose = Pose::new(mood, frame);
    let paint = |cell: u8| match cell {
        b'B' => Some(color),
        b'D' => Some(color.shaded(-0.35)),
        b'L' => Some(color.shaded(0.35)),
        b'F' => Some(palette::FACE),
        b'P' => Some(palette::PINK),
        _ => None,
    };

    let mut pixels = Pixels::new(SPRITE_SIZE, SPRITE_SIZE);
    let (ox, oy) = (BODY_X + pose.dx, BODY_Y + pose.dy);
    for (y, row) in template.body.iter().enumerate() {
        let walking = template.walk.as_ref().filter(|walk| pose.walk && (walk.row..walk.row + walk.rows.len()).contains(&y));
        let row = walking.map_or(row, |walk| &walk.rows[y - walk.row]);
        for (x, cell) in row.bytes().enumerate() {
            if let Some(c) = paint(cell) {
                pixels.set(ox + x as i32, oy + y as i32, c);
            }
        }
    }

    let eye_color = template.eye_color.as_deref().and_then(Rgb::from_hex).unwrap_or(palette::EYE);
    for (ex, mirrored) in [(template.eyes.left, false), (template.eyes.right, true)] {
        for &(dx, dy) in eye_cells(pose.eyes, mirrored) {
            pixels.set(ox + ex + dx, oy + template.eyes.y + dy, eye_color);
        }
    }

    for overlay in &pose.overlays {
        for (y, row) in overlay.glyph.rows().iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'X' {
                    pixels.set(overlay.x + x as i32, overlay.y + y as i32, overlay.color);
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: Rgb = Rgb::new(0xff, 0x6b, 0x1a);

    #[test]
    fn hex_colors() {
        assert_eq!(Rgb::from_hex("#ff6b1a"), Some(ORANGE));
        assert_eq!(Rgb::from_hex("ff6b1a"), None);
        assert_eq!(Rgb::from_hex("#ff6b1"), None);
        assert_eq!(Rgb::from_hex("#+f6b1a"), None);
        assert_eq!(Rgb::from_hex("#gggggg"), None);
    }

    #[test]
    fn shading_goes_toward_black_or_white() {
        assert_eq!(Rgb::new(100, 200, 50).shaded(-0.5), Rgb::new(50, 100, 25));
        assert_eq!(Rgb::new(100, 200, 50).shaded(1.0), Rgb::new(255, 255, 255));
        assert_eq!(ORANGE.shaded(0.0), ORANGE);
    }

    #[test]
    fn every_template_is_16_by_16_with_its_eyes_on_the_body() {
        assert_eq!(characters()[0].id, "default");
        assert!(characters().len() >= 4);
        for c in characters() {
            assert_eq!(c.body.len(), 16, "{}", c.id);
            assert!(c.body.iter().all(|row| row.len() == 16), "{}", c.id);
            if let Some(walk) = &c.walk {
                assert!(walk.row + walk.rows.len() <= 16, "{}", c.id);
                assert!(walk.rows.iter().all(|row| row.len() == 16), "{}", c.id);
            }
        }
    }

    #[test]
    fn an_unknown_character_is_the_default() {
        assert_eq!(character("nope").id, "default");
        assert_eq!(character("robot").id, "robot");
    }

    #[test]
    fn the_body_sits_at_the_offset_in_the_color_of_the_session() {
        let pixels = sprite_frame("default", ORANGE, Mood::Idle, 1);
        // Row 4 of the template is ...BBBBBBBBBB...
        assert_eq!(pixels.get(BODY_X + 3, BODY_Y + 4), Some(ORANGE));
        assert_eq!(pixels.get(BODY_X + 2, BODY_Y + 4), None);
        // Row 10 is the darker underside.
        assert_eq!(pixels.get(BODY_X + 3, BODY_Y + 10), Some(ORANGE.shaded(-0.35)));
        assert_eq!((pixels.width, pixels.height), (SPRITE_SIZE, SPRITE_SIZE));
    }

    #[test]
    fn eyes_blink_once_every_sixteen_frames() {
        let eye = |frame| sprite_frame("default", ORANGE, Mood::Idle, frame).get(BODY_X + 5, BODY_Y + 5);
        assert_eq!(eye(1), Some(palette::EYE));
        assert_eq!(eye(0), Some(ORANGE), "closed: only the lower row of the eye");
        assert_eq!(eye(16), Some(ORANGE));
    }

    #[test]
    fn a_working_creature_bobs_and_shows_three_dots() {
        let frame = |n| sprite_frame("default", ORANGE, Mood::Working, n);
        assert_ne!(frame(0), frame(1));
        assert_eq!(frame(0), frame(12), "the animation repeats");
        assert_eq!(frame(0).get(17, 3), Some(palette::DOT_ON), "the first dot is raised");
        assert_eq!(frame(0).get(19, 4), Some(palette::DOT_ON));
        assert_eq!(frame(1).get(19, 3), Some(palette::DOT_ON), "then the second");
    }

    #[test]
    fn each_mood_leaves_its_mark() {
        let has = |mood, x, y, color| sprite_frame("default", ORANGE, mood, 0).get(x, y) == Some(color);
        assert!(has(Mood::Alert, 21, 0, palette::BANG));
        assert!(has(Mood::Asking, 19, 0, palette::QUESTION));
        assert!(has(Mood::Error, 19, 1, palette::CROSS));
        assert!(has(Mood::Finished, 22, 0, palette::CHECK));
        assert!(sprite_frame("default", ORANGE, Mood::Sleeping, 6).filled() < sprite_frame("default", ORANGE, Mood::Sleeping, 0).filled());
    }

    #[test]
    fn a_robot_has_its_own_eye_color() {
        let pixels = sprite_frame("robot", ORANGE, Mood::Idle, 1);
        assert_eq!(pixels.get(BODY_X + 5, BODY_Y + 4), Some(Rgb::from_hex("#9ff3ff").unwrap()));
    }

    #[test]
    fn trimming_keeps_only_the_filled_box() {
        let mut p = Pixels::new(6, 6);
        p.set(2, 1, ORANGE);
        p.set(4, 3, ORANGE);
        let t = p.trimmed();
        assert_eq!((t.width, t.height), (3, 3));
        assert_eq!(t.get(0, 0), Some(ORANGE));
        assert_eq!(t.get(2, 2), Some(ORANGE));
        assert_eq!(Pixels::new(4, 4).trimmed().width, 0);
    }
}
