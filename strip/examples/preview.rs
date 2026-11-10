//! Draws a sample strip into a PNG, to look at the drawing without a window:
//!   cargo run --example preview -- out.png [ui-scale]

use winsill_strip::appearance::Appearance;
use winsill_strip::layout::{Edge, Layout};
use winsill_strip::protocol::{Icon, Session, State};
use winsill_strip::raster::Canvas;
use winsill_strip::view::{Scene, render};

fn session(id: &str, character: &str, color: &str, state: State, pinned: bool, live: bool) -> Session {
    Session {
        id: id.into(),
        name: id.into(),
        cwd: String::new(),
        state,
        activity: None,
        live,
        state_since: 0,
        pinned,
        position: None,
        icon: Icon { character: character.into(), color: color.into() },
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "preview.png".into());
    let ui: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let sessions = [
        session("a", "default", "#d97757", State::Working, true, true),
        session("b", "robot", "#4f7cff", State::NeedsPermission, true, true),
        session("c", "ghost", "#a66cff", State::NeedsInput, true, true),
        session("d", "cat", "#ffd23f", State::Compacting, true, true),
        session("e", "default", "#4cd964", State::Done, true, true),
        session("f", "robot", "#ff5fa2", State::Error, true, true),
        session("g", "ghost", "#3ccfe6", State::Idle, true, true),
        session("h", "cat", "#e8e8e8", State::Ended, true, false),
    ];
    let refs: Vec<&Session> = sessions.iter().collect();
    // Six frames of the animation, side by side, over a desktop-ish blue.
    let layout = Layout::fitting(sessions.len() + 1, (900.0 * ui) as i32, ui);
    let (w, h) = layout.size();
    let frames = 6;
    let gap = (12.0 * ui) as i32;
    let mut sheet = Canvas::new(frames * (w + gap) + gap, h + 2 * gap);
    for px in sheet.bgra.chunks_mut(4) {
        px.copy_from_slice(&[0x63, 0x4a, 0x3a, 0xff]);
    }
    for frame in 0..frames {
        let creatures: Vec<Appearance> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| Appearance::session(s, i == 0, i == 2, i64::from(frame), 0))
            .chain([Appearance::add_button(false)])
            .collect();
        let tab = Appearance::tab(&refs, false, Edge::Right, false, i64::from(frame), 0);
        let mut canvas = Canvas::new(w, h);
        render(&mut canvas, &Scene { layout: &layout, edge: Edge::Right, offset: 0, ui, creatures: &creatures, tab: &tab });
        let (x0, y0) = (gap + frame * (w + gap), gap);
        for y in 0..h {
            for x in 0..w {
                let p = canvas.pixel(x, y);
                let a = f32::from(p[3]) / 255.0;
                let i = ((y0 + y) as usize * sheet.width as usize + (x0 + x) as usize) * 4;
                for c in 0..3 {
                    sheet.bgra[i + c] = (f32::from(p[c]) + f32::from(sheet.bgra[i + c]) * (1.0 - a)).round() as u8;
                }
            }
        }
    }
    std::fs::write(&out, png(sheet.width as u32, sheet.height as u32, &sheet.bgra)).expect("cannot write the PNG");
    println!("{out}: {}x{}", sheet.width, sheet.height);
}

/// An uncompressed PNG: big, but it needs no library and opens everywhere.
fn png(width: u32, height: u32, bgra: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in bgra.chunks(width as usize * 4) {
        raw.push(0);
        for p in row.chunks(4) {
            raw.extend_from_slice(&[p[2], p[1], p[0]]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    for (kind, data) in [(b"IHDR", header), (b"IDAT", z), (b"IEND", Vec::new())] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut chunk = kind.to_vec();
        chunk.extend_from_slice(&data);
        out.extend_from_slice(&chunk);
        out.extend_from_slice(&crc32(&chunk).to_be_bytes());
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
