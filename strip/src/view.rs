//! Draws the strip: its backdrop, the tab and one cell per creature, into a canvas the size of the window.

use crate::appearance::{Appearance, Paint, Tile};
use crate::layout::{Edge, Layout, Rect};
use crate::raster::{Canvas, Corners};
use crate::sprite::Rgb;

/// The dark glass behind everything.
pub const BACKDROP: Paint = Paint { rgb: Rgb::new(13, 13, 13), alpha: 0.78 };
pub const LINE: Paint = Paint::white(0.1);
pub const TEXT: Rgb = Rgb::new(245, 245, 245);
pub const MUTED: Rgb = Rgb::new(153, 153, 153);

pub struct Scene<'a> {
    pub layout: &'a Layout,
    pub edge: Edge,
    /// How far the strip and tab are slid toward the edge, in device pixels: 0 open, `collapse_offset` collapsed.
    pub offset: i32,
    /// The DPI factor (1.0 at 96 DPI).
    pub ui: f32,
    /// One per cell: the sessions, then the add button. Empty while the strip is collapsed.
    pub creatures: &'a [Appearance],
    pub tab: &'a Appearance,
}

/// Draws `scene` over a cleared `canvas`, whose size is the window's.
pub fn render(canvas: &mut Canvas, scene: &Scene) {
    let (layout, edge, ui) = (scene.layout, scene.edge, scene.ui);
    let height = canvas.height;
    let corners = Corners::inner(edge);
    let radius = 3.0 * layout.scale as f32;
    let line = ui.max(1.0);

    let strip = layout.strip_rect(height, edge).offset(scene.offset, 0);
    canvas.fill_rrect(strip, 10.0 * ui, corners, BACKDROP);
    canvas.stroke_rrect(strip, 10.0 * ui, corners, line, LINE);

    let tab = layout.tab_rect(height, edge, false).offset(scene.offset, 0);
    canvas.fill_rrect(tab, radius, corners, BACKDROP);
    canvas.stroke_rrect(tab, radius, corners, line, LINE);
    draw_cell(canvas, scene.tab, tab, tab, layout, radius, corners, ui);

    for (index, creature) in scene.creatures.iter().enumerate() {
        let cell = layout.cell_frame(index, strip);
        draw_cell(canvas, creature, layout.tile_rect(cell), layout.sprite_rect(cell), layout, radius, Corners::ALL, ui);
    }
}

/// A tile with a sprite on it: a creature, the add button or the tab.
fn draw_cell(canvas: &mut Canvas, a: &Appearance, tile: Rect, sprite: Rect, layout: &Layout, radius: f32, corners: Corners, ui: f32) {
    let Tile { fill, border, border_width } = a.tile;
    if let Some(fill) = fill {
        canvas.fill_rrect(tile, radius, corners, fill);
    }
    if let Some(border) = border {
        canvas.stroke_rrect(tile, radius, corners, border_width * ui, border);
    }
    canvas.draw_sprite(&a.pixels, sprite, layout.scale, a.opacity);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::State;
    use crate::sessions::tests::session;

    #[test]
    fn a_strip_draws_glass_behind_its_cells_and_nothing_outside() {
        let layout = Layout::fitting(2, 1000, 1.0);
        let (w, h) = layout.size();
        let mut canvas = Canvas::new(w, h);
        let sessions = [session("a", "api", State::Working)];
        let creatures: Vec<_> = sessions.iter().map(|s| Appearance::session(s, false, false, 0, 0)).chain([Appearance::add_button(false)]).collect();
        let tab = Appearance::tab(&[], false, Edge::Right, false, 0, 0);
        render(&mut canvas, &Scene { layout: &layout, edge: Edge::Right, offset: 0, ui: 1.0, creatures: &creatures, tab: &tab });

        let strip = layout.strip_rect(h, Edge::Right);
        assert!(canvas.pixel(strip.x + 30, strip.bottom() - 3)[3] > 150, "the backdrop is on, and mostly opaque");
        assert_eq!(canvas.pixel(0, 0)[3], 0, "beside the tab, the window is clear and lets clicks through");
        assert!(canvas.bgra.chunks(4).any(|p| p[3] == 255), "the creature is solid");
    }

    #[test]
    fn collapsed_only_the_tab_is_left_on_screen() {
        let layout = Layout::fitting(2, 1000, 1.0);
        let (w, h) = layout.size();
        let mut canvas = Canvas::new(w, h);
        let tab = Appearance::tab(&[], true, Edge::Right, false, 0, 0);
        let offset = layout.collapse_offset(Edge::Right);
        render(&mut canvas, &Scene { layout: &layout, edge: Edge::Right, offset, ui: 1.0, creatures: &[], tab: &tab });
        let (strip_w, _) = layout.strip_size();
        let covered = |x0: i32, x1: i32| (0..h).any(|y| (x0..x1).any(|x| canvas.pixel(x, y)[3] > 0));
        assert!(!covered(0, strip_w), "the strip's place is empty");
        assert!(covered(strip_w + 1, w), "the tab is against the edge");
    }
}
