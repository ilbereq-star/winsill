//! Where everything sits, in device pixels with the origin at the top left.
//!
//! The window holds the strip against the screen edge and the tab beside it, on the strip's inner
//! side. Collapsing slides both toward the edge by `collapse_offset`, which leaves only the tab on
//! screen. Sprite pixels are scaled by a whole number of device pixels, so they stay crisp at any
//! DPI.

use crate::sprite::SPRITE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub fn mid_y(self) -> i32 {
        self.y + self.h / 2
    }

    pub fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    pub fn offset(self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn inset(self, d: i32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2 * d, self.h - 2 * d)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

impl Edge {
    pub fn name(self) -> &'static str {
        match self {
            Edge::Left => "left",
            Edge::Right => "right",
        }
    }

    pub fn parse(text: &str) -> Option<Edge> {
        match text {
            "left" => Some(Edge::Left),
            "right" => Some(Edge::Right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Cell(usize),
    Tab,
}

/// Sprite pixels: the collapse tab is this big.
pub const TAB_PIXELS: (i32, i32) = (8, 21);
const CELL_PADDING_PIXELS: i32 = 2;
const CELL_PIXELS: i32 = SPRITE_SIZE as i32 + 2 * CELL_PADDING_PIXELS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Cells in the strip: the sessions and the add button.
    pub count: usize,
    /// Device pixels per sprite pixel.
    pub scale: i32,
    /// Space around the cells, in device pixels.
    pub inset: i32,
    /// Between the strip and a label beside it, in device pixels.
    pub label_gap: i32,
}

impl Layout {
    /// The biggest whole scale at which `count` cells fit in `height` device pixels, down to 1,
    /// past which the strip overflows and `frame` cuts it to the screen. `ui` is the DPI factor
    /// (1.0 at 96 DPI).
    pub fn fitting(count: usize, height: i32, ui: f32) -> Layout {
        let inset = (4.0 * ui).round() as i32;
        let largest = ((2.0 * ui).floor() as i32).max(1);
        let needed = |scale: i32| count as i32 * CELL_PIXELS * scale + 2 * inset;
        let scale = (1..=largest).rev().find(|&scale| needed(scale) <= height).unwrap_or(1);
        Layout { count, scale, inset, label_gap: (6.0 * ui).round() as i32 }
    }

    pub fn cell_side(&self) -> i32 {
        self.scale * CELL_PIXELS
    }

    pub fn strip_size(&self) -> (i32, i32) {
        (self.cell_side() + 2 * self.inset, self.count as i32 * self.cell_side() + 2 * self.inset)
    }

    pub fn tab_size(&self) -> (i32, i32) {
        (TAB_PIXELS.0 * self.scale, TAB_PIXELS.1 * self.scale)
    }

    pub fn size(&self) -> (i32, i32) {
        (self.strip_size().0 + self.tab_size().0, self.strip_size().1)
    }

    /// The window, in screen coordinates: against `work`'s edge, centered on it, and cut to its height.
    pub fn frame(&self, work: Rect, edge: Edge) -> Rect {
        let (w, h) = self.size();
        let h = h.min(work.h);
        let x = match edge {
            Edge::Right => work.right() - w,
            Edge::Left => work.x,
        };
        Rect::new(x, work.y + (work.h - h) / 2, w, h)
    }

    /// The rest of this block takes the window's height, which `frame` may have cut.
    pub fn strip_rect(&self, height: i32, edge: Edge) -> Rect {
        let (strip_w, _) = self.strip_size();
        let x = match edge {
            Edge::Right => self.size().0 - strip_w,
            Edge::Left => 0,
        };
        Rect::new(x, 0, strip_w, height)
    }

    pub fn tab_rect(&self, height: i32, edge: Edge, collapsed: bool) -> Rect {
        let (w, h) = self.tab_size();
        let x = match edge {
            Edge::Right => 0,
            Edge::Left => self.size().0 - w,
        };
        let open = Rect::new(x, (height - h) / 2, w, h);
        if collapsed { open.offset(self.collapse_offset(edge), 0) } else { open }
    }

    pub fn collapse_offset(&self, edge: Edge) -> i32 {
        match edge {
            Edge::Right => self.strip_size().0,
            Edge::Left => -self.strip_size().0,
        }
    }

    pub fn hit(&self, px: i32, py: i32, height: i32, edge: Edge, collapsed: bool) -> Option<Hit> {
        if self.tab_rect(height, edge, collapsed).contains(px, py) {
            return Some(Hit::Tab);
        }
        let strip = self.strip_rect(height, edge);
        if collapsed || !strip.contains(px, py) || py < self.inset {
            return None;
        }
        let index = ((py - self.inset) / self.cell_side()) as usize;
        (index < self.count).then_some(Hit::Cell(index))
    }

    /// A cell of the strip, given the strip's own rectangle (which may be slid aside).
    pub fn cell_frame(&self, index: usize, strip: Rect) -> Rect {
        let side = self.cell_side();
        Rect::new(strip.x + self.inset, strip.y + self.inset + index as i32 * side, side, side)
    }

    /// The tile a creature stands on.
    pub fn tile_rect(&self, cell: Rect) -> Rect {
        cell.inset(self.scale)
    }

    /// Where the 24×24 sprite goes.
    pub fn sprite_rect(&self, cell: Rect) -> Rect {
        cell.inset(CELL_PADDING_PIXELS * self.scale)
    }

    /// Beside `item`'s middle, facing into the screen and clear of `outside`; all in screen coordinates.
    pub fn label_frame(&self, size: (i32, i32), item: Rect, outside: Rect, edge: Edge) -> Rect {
        let x = match edge {
            Edge::Right => outside.x - self.label_gap - size.0,
            Edge::Left => outside.right() + self.label_gap,
        };
        Rect::new(x, item.mid_y() - size.1 / 2, size.0, size.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_is_the_biggest_whole_one_that_fits() {
        // 12 cells at scale 2: 12 * 28 * 2 + 8 = 680
        assert_eq!(Layout::fitting(12, 728, 1.0).scale, 2);
        assert_eq!(Layout::fitting(14, 728, 1.0).scale, 1);
        assert_eq!(Layout::fitting(14, 1456, 2.0).scale, 3, "a 200% screen has twice the pixels");
        assert_eq!(Layout::fitting(3, 1000, 2.0).scale, 4);
        assert_eq!(Layout::fitting(100, 300, 1.0).scale, 1, "overflows at the smallest scale");
    }

    #[test]
    fn sizes() {
        let layout = Layout::fitting(3, 1000, 1.0);
        assert_eq!(layout.cell_side(), 56);
        assert_eq!(layout.strip_size(), (64, 176));
        assert_eq!(layout.tab_size(), (16, 42));
        assert_eq!(layout.size(), (80, 176));
    }

    #[test]
    fn the_window_hugs_the_edge_and_centers_on_the_work_area() {
        let layout = Layout::fitting(3, 1000, 1.0);
        let work = Rect::new(0, 0, 1366, 728);
        assert_eq!(layout.frame(work, Edge::Right), Rect::new(1286, 276, 80, 176));
        assert_eq!(layout.frame(work, Edge::Left), Rect::new(0, 276, 80, 176));
        let tall = Layout::fitting(100, 300, 1.0);
        assert_eq!(tall.frame(work, Edge::Right).h, 728, "cut to the screen");
        let second_monitor = Rect::new(1920, 40, 1920, 1000);
        assert_eq!(layout.frame(second_monitor, Edge::Right).x, 3840 - 80);
    }

    #[test]
    fn the_strip_is_on_the_edge_side_and_the_tab_inside() {
        let layout = Layout::fitting(3, 1000, 1.0);
        assert_eq!(layout.strip_rect(176, Edge::Right), Rect::new(16, 0, 64, 176));
        assert_eq!(layout.tab_rect(176, Edge::Right, false), Rect::new(0, 67, 16, 42));
        assert_eq!(layout.strip_rect(176, Edge::Left), Rect::new(0, 0, 64, 176));
        assert_eq!(layout.tab_rect(176, Edge::Left, false), Rect::new(64, 67, 16, 42));
    }

    #[test]
    fn collapsing_leaves_only_the_tab_against_the_edge() {
        let layout = Layout::fitting(3, 1000, 1.0);
        let (w, _) = layout.size();
        let right = layout.tab_rect(176, Edge::Right, true);
        assert_eq!(right.right(), w);
        let left = layout.tab_rect(176, Edge::Left, true);
        assert_eq!(left.x, 0);
    }

    #[test]
    fn hits() {
        let layout = Layout::fitting(3, 1000, 1.0);
        let hit = |x, y, collapsed| layout.hit(x, y, 176, Edge::Right, collapsed);
        assert_eq!(hit(5, 80, false), Some(Hit::Tab));
        assert_eq!(hit(40, 10, false), Some(Hit::Cell(0)));
        assert_eq!(hit(40, 4 + 56, false), Some(Hit::Cell(1)));
        assert_eq!(hit(40, 4 + 56 * 3 - 1, false), Some(Hit::Cell(2)));
        assert_eq!(hit(40, 2, false), None, "the inset above the first cell");
        assert_eq!(hit(40, 4 + 56 * 3, false), None, "past the last cell");
        assert_eq!(hit(40, 10, true), None, "collapsed, the strip is off screen");
        assert_eq!(hit(5, 80, true), None, "the tab moved with it");
        assert_eq!(hit(70, 80, true), Some(Hit::Tab));
    }

    #[test]
    fn cells_and_their_insides() {
        let layout = Layout::fitting(3, 1000, 1.0);
        let strip = layout.strip_rect(176, Edge::Right);
        let cell = layout.cell_frame(1, strip);
        assert_eq!(cell, Rect::new(20, 60, 56, 56));
        assert_eq!(layout.tile_rect(cell), Rect::new(22, 62, 52, 52));
        assert_eq!(layout.sprite_rect(cell), Rect::new(24, 64, 48, 48));
    }

    #[test]
    fn a_label_faces_into_the_screen() {
        let layout = Layout::fitting(3, 1000, 1.0);
        let frame = Rect::new(1286, 276, 80, 176);
        let cell = Rect::new(1306, 336, 56, 56);
        assert_eq!(layout.label_frame((100, 30), cell, frame, Edge::Right), Rect::new(1286 - 6 - 100, 364 - 15, 100, 30));
        let left = Rect::new(0, 276, 80, 176);
        assert_eq!(layout.label_frame((100, 30), cell, left, Edge::Left).x, 86);
    }
}
