//! A tiny software rasterizer: the strip is a few rounded rectangles and some scaled sprites, so
//! drawing them ourselves into a 32-bit buffer costs less memory than any graphics library would.
//! The buffer is premultiplied BGRA, which is what a layered window wants.

use crate::appearance::Paint;
use crate::layout::{Edge, Rect};
use crate::sprite::{Pixels, Rgb};

/// Which corners of a rectangle are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corners {
    pub top_left: bool,
    pub top_right: bool,
    pub bottom_left: bool,
    pub bottom_right: bool,
}

impl Corners {
    pub const ALL: Corners = Corners { top_left: true, top_right: true, bottom_left: true, bottom_right: true };

    /// The corners on the side facing into the screen, which is where the strip is rounded.
    pub fn inner(edge: Edge) -> Corners {
        let left = edge == Edge::Right;
        Corners { top_left: left, bottom_left: left, top_right: !left, bottom_right: !left }
    }
}

pub struct Canvas {
    pub width: i32,
    pub height: i32,
    /// Premultiplied BGRA, top row first.
    pub bgra: Vec<u8>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Canvas {
        let (width, height) = (width.max(0), height.max(0));
        Canvas { width, height, bgra: vec![0; width as usize * height as usize * 4] }
    }

    /// Fully transparent again; keeps the buffer when the size is the same.
    pub fn reset(&mut self, width: i32, height: i32) {
        let (width, height) = (width.max(0), height.max(0));
        self.width = width;
        self.height = height;
        self.bgra.clear();
        self.bgra.resize(width as usize * height as usize * 4, 0);
    }

    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let i = self.index(x, y).expect("pixel outside the canvas");
        [self.bgra[i], self.bgra[i + 1], self.bgra[i + 2], self.bgra[i + 3]]
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then(|| (y as usize * self.width as usize + x as usize) * 4)
    }

    /// Lays `rgb` over what is there, `alpha` thick (0…1). Outside the canvas is ignored.
    pub fn blend(&mut self, x: i32, y: i32, rgb: Rgb, alpha: f32) {
        let Some(i) = self.index(x, y) else { return };
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let keep = 1.0 - alpha;
        let over = |new: u8, old: u8| (f32::from(new) * alpha + f32::from(old) * keep).round().min(255.0) as u8;
        self.bgra[i] = over(rgb.b, self.bgra[i]);
        self.bgra[i + 1] = over(rgb.g, self.bgra[i + 1]);
        self.bgra[i + 2] = over(rgb.r, self.bgra[i + 2]);
        self.bgra[i + 3] = (alpha * 255.0 + f32::from(self.bgra[i + 3]) * keep).round().min(255.0) as u8;
    }

    /// A solid block of device pixels.
    pub fn fill_rect(&mut self, rect: Rect, paint: Paint) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.blend(x, y, paint.rgb, paint.alpha);
            }
        }
    }

    /// A rounded rectangle, smooth at its corners.
    pub fn fill_rrect(&mut self, rect: Rect, radius: f32, corners: Corners, paint: Paint) {
        self.shape(rect, |d| (0.5 - d).clamp(0.0, 1.0), radius, corners, paint);
    }

    /// The outline of a rounded rectangle, drawn inside its bounds, `width` thick.
    pub fn stroke_rrect(&mut self, rect: Rect, radius: f32, corners: Corners, width: f32, paint: Paint) {
        let ring = |d: f32| (0.5 - d).clamp(0.0, 1.0) - (0.5 - (d + width)).clamp(0.0, 1.0);
        self.shape(rect, ring, radius, corners, paint);
    }

    // `coverage` turns a pixel's distance from the shape's edge (negative inside) into 0…1.
    fn shape(&mut self, rect: Rect, coverage: impl Fn(f32) -> f32, radius: f32, corners: Corners, paint: Paint) {
        let (half_w, half_h) = (rect.w as f32 / 2.0, rect.h as f32 / 2.0);
        let (cx, cy) = (rect.x as f32 + half_w, rect.y as f32 + half_h);
        let radius = radius.max(0.0).min(half_w).min(half_h);
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let rounded = match (py < cy, px < cx) {
                    (true, true) => corners.top_left,
                    (true, false) => corners.top_right,
                    (false, true) => corners.bottom_left,
                    (false, false) => corners.bottom_right,
                };
                let r = if rounded { radius } else { 0.0 };
                let (qx, qy) = ((px - cx).abs() - (half_w - r), (py - cy).abs() - (half_h - r));
                let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                let c = coverage(distance);
                if c > 0.0 {
                    self.blend(x, y, paint.rgb, paint.alpha * c);
                }
            }
        }
    }

    /// `pixels` at `rect`'s corner, each pixel a `scale`×`scale` block, `opacity` thick.
    pub fn draw_sprite(&mut self, pixels: &Pixels, rect: Rect, scale: i32, opacity: f32) {
        for sy in 0..pixels.height as i32 {
            for sx in 0..pixels.width as i32 {
                if let Some(color) = pixels.get(sx, sy) {
                    let block = Rect::new(rect.x + sx * scale, rect.y + sy * scale, scale, scale);
                    self.fill_rect(block, Paint { rgb: color, alpha: opacity });
                }
            }
        }
    }

    /// A `w`×`h` coverage mask (one byte per pixel, 0…255), such as antialiased text, in one color.
    pub fn draw_mask(&mut self, x: i32, y: i32, w: i32, h: i32, mask: &[u8], rgb: Rgb, alpha: f32) {
        for row in 0..h {
            for col in 0..w {
                let coverage = mask[(row * w + col) as usize];
                if coverage > 0 {
                    self.blend(x + col, y + row, rgb, alpha * f32::from(coverage) / 255.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Paint = Paint::white(1.0);

    #[test]
    fn a_new_canvas_is_transparent() {
        let canvas = Canvas::new(3, 2);
        assert_eq!(canvas.bgra.len(), 24);
        assert!(canvas.bgra.iter().all(|&b| b == 0));
    }

    #[test]
    fn blending_is_premultiplied_and_stacks() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend(0, 0, Rgb::new(255, 0, 0), 0.5);
        assert_eq!(canvas.pixel(0, 0), [0, 0, 128, 128], "half-red on nothing is half-covered, premultiplied");
        canvas.blend(0, 0, Rgb::new(255, 0, 0), 1.0);
        assert_eq!(canvas.pixel(0, 0), [0, 0, 255, 255]);
        canvas.blend(5, 5, Rgb::new(1, 2, 3), 1.0); // outside: ignored
    }

    #[test]
    fn a_sprite_is_scaled_by_whole_blocks() {
        let mut pixels = Pixels::new(2, 1);
        pixels.set(1, 0, Rgb::new(0, 255, 0));
        let mut canvas = Canvas::new(8, 4);
        canvas.draw_sprite(&pixels, Rect::new(1, 1, 6, 3), 3, 1.0);
        assert_eq!(canvas.pixel(3, 1)[3], 0, "the empty pixel stays clear");
        assert_eq!(canvas.pixel(4, 1), [0, 255, 0, 255]);
        assert_eq!(canvas.pixel(6, 3), [0, 255, 0, 255]);
        assert_eq!(canvas.pixel(7, 3)[3], 0);
        assert_eq!(canvas.pixel(4, 4 - 1)[3], 255);
    }

    #[test]
    fn a_rounded_rectangle_is_solid_inside_and_clear_at_its_rounded_corners() {
        let mut canvas = Canvas::new(20, 20);
        canvas.fill_rrect(Rect::new(0, 0, 20, 20), 8.0, Corners::ALL, WHITE);
        assert_eq!(canvas.pixel(10, 10)[3], 255);
        assert_eq!(canvas.pixel(0, 0)[3], 0, "the corner is cut");
        assert!(canvas.pixel(2, 2)[3] > 0 && canvas.pixel(2, 2)[3] < 255, "and its edge is smooth");
        assert_eq!(canvas.pixel(10, 0)[3], 255, "straight edges stay solid");
    }

    #[test]
    fn only_the_chosen_corners_are_rounded() {
        let mut canvas = Canvas::new(20, 20);
        canvas.fill_rrect(Rect::new(0, 0, 20, 20), 8.0, Corners::inner(Edge::Right), WHITE);
        assert_eq!(canvas.pixel(0, 0)[3], 0, "left corner rounded");
        assert_eq!(canvas.pixel(19, 0)[3], 255, "right corner square");
        assert_eq!(canvas.pixel(19, 19)[3], 255);
    }

    #[test]
    fn a_stroke_is_a_ring_inside_the_bounds() {
        let mut canvas = Canvas::new(20, 20);
        canvas.stroke_rrect(Rect::new(0, 0, 20, 20), 4.0, Corners::ALL, 2.0, WHITE);
        assert_eq!(canvas.pixel(10, 0)[3], 255);
        assert_eq!(canvas.pixel(10, 1)[3], 255);
        assert_eq!(canvas.pixel(10, 2)[3], 0, "the middle is empty");
        assert_eq!(canvas.pixel(10, 10)[3], 0);
        assert_eq!(canvas.pixel(0, 10)[3], 255);
    }

    #[test]
    fn a_mask_draws_text_shaped_coverage() {
        let mut canvas = Canvas::new(3, 1);
        canvas.draw_mask(0, 0, 3, 1, &[0, 128, 255], Rgb::new(255, 255, 255), 1.0);
        assert_eq!(canvas.pixel(0, 0)[3], 0);
        assert_eq!(canvas.pixel(1, 0)[3], 128);
        assert_eq!(canvas.pixel(2, 0)[3], 255);
    }
}
