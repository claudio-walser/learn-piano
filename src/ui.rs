//! Drawing helpers, colors and touch/mouse handling.

use macroquad::models::{draw_mesh, Mesh, Vertex};
use std::sync::atomic::{AtomicU32, Ordering};

use macroquad::prelude::*;

pub const fn rgb(c: u32) -> Color {
    Color::new(
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
        1.0,
    )
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub const BG: Color = rgb(0x1b1d29);
pub const BG_DEEP: Color = rgb(0x12131b);
pub const PANEL: Color = rgb(0x262938);
pub const PANEL_HI: Color = rgb(0x33374a);
pub const TEXT: Color = rgb(0xffffff);
pub const MUTED: Color = rgb(0x7c8b93);
pub const RIGHT_HAND: Color = rgb(0x4fc3f7);
pub const LEFT_HAND: Color = rgb(0xffa94d);
pub const GOOD: Color = rgb(0x55e00f);
pub const BAD: Color = rgb(0xe5393b);
pub const STAR_GOLD: Color = rgb(0xffc400);

/// Color per difficulty position: green, yellow, red, then extras.
pub fn difficulty_color(index: usize) -> Color {
    // Order of compositions.xml: prep, beginner, intermediate, expert, …
    const C: [Color; 6] = [
        rgb(0x4fc3f7),
        rgb(0x55e00f),
        rgb(0xffc400),
        rgb(0xe01b24),
        rgb(0xb36bff),
        rgb(0xff8a3d),
    ];
    C[index % C.len()]
}

// ------------------------------------------------------------ UI scaling
//
// The UI is laid out on a virtual canvas of at least 1280 x 800 units that is
// scaled to the window. Text is rasterized at physical resolution.

const REF_W: f32 = 1280.0;
const REF_H: f32 = 800.0;

static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32

/// Mouse wheel movement this frame in notches (+ = up). X11 reports ±1 per
/// notch, Wayland about 10–15 per notch.
pub fn wheel_notches() -> f32 {
    let y = mouse_wheel().1;
    if y.abs() > 2.0 { y / 10.0 } else { y }
}

pub fn scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// Virtual canvas width.
pub fn vw() -> f32 {
    screen_width() / scale()
}

/// Virtual canvas height.
pub fn vh() -> f32 {
    screen_height() / scale()
}

/// Sets up the camera for this frame's virtual canvas.
pub fn begin_frame() {
    let (w, h) = (screen_width(), screen_height());
    let s = (w / REF_W).min(h / REF_H).max(0.2);
    SCALE.store(s.to_bits(), Ordering::Relaxed);
    let (cw, ch) = (w / s, h / s);
    set_camera(&Camera2D {
        target: vec2(cw / 2.0, ch / 2.0),
        zoom: vec2(2.0 / cw, 2.0 / ch),
        ..Default::default()
    });
}

pub struct Fonts {
    pub regular: Font,
    pub bold: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let mut regular = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/Inter-Regular.ttf"))
            .expect("bundled font");
        let mut bold = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/Inter-Bold.ttf"))
            .expect("bundled font");
        regular.set_filter(FilterMode::Linear);
        bold.set_filter(FilterMode::Linear);
        Fonts { regular, bold }
    }
}

fn raster_size(size: f32) -> (u16, f32) {
    let px = (size * scale()).round().max(1.0);
    (px as u16, size / px)
}

pub fn text_width(font: &Font, s: &str, size: f32) -> f32 {
    let (px, k) = raster_size(size);
    measure_text(s, Some(font), px, k).width
}

/// Draws text with its left edge at `x`, vertically centered on `cy`.
pub fn text_left(font: &Font, s: &str, x: f32, cy: f32, size: f32, color: Color) {
    let (px, k) = raster_size(size);
    let cap = size * 0.72; // approx. cap height of Inter
    draw_text_ex(
        s,
        x,
        cy + cap / 2.0,
        TextParams { font: Some(font), font_size: px, font_scale: k, color, ..Default::default() },
    );
}

pub fn text_center(font: &Font, s: &str, cx: f32, cy: f32, size: f32, color: Color) {
    let w = text_width(font, s, size);
    text_left(font, s, cx - w / 2.0, cy, size, color);
}

/// Largest font size (<= `size`) at which `s` fits into `max_w`.
pub fn fit_size(font: &Font, s: &str, size: f32, max_w: f32) -> f32 {
    let w = text_width(font, s, size);
    if w <= max_w || w <= 0.0 {
        size
    } else {
        (size * max_w / w).floor().max(8.0)
    }
}

pub fn rounded_rect(r: Rect, radius: f32, c: Color) {
    let rad = radius.min(r.w * 0.5).min(r.h * 0.5).max(0.0);
    if rad < 1.0 {
        draw_rectangle(r.x, r.y, r.w, r.h, c);
        return;
    }
    // Fewer segments for small corners: keys and notes are drawn by the hundred.
    let seg: usize = if rad < 5.0 { 2 } else if rad < 10.0 { 3 } else { 5 };
    let mut v = Vec::with_capacity(4 * (seg + 1) + 1);
    let center = r.center();
    v.push(Vertex::new(center.x, center.y, 0.0, 0.0, 0.0, c));
    let corners = [
        (r.x + r.w - rad, r.y + r.h - rad, 0.0f32),
        (r.x + rad, r.y + r.h - rad, 90.0),
        (r.x + rad, r.y + rad, 180.0),
        (r.x + r.w - rad, r.y + rad, 270.0),
    ];
    for (cx, cy, a0) in corners {
        for i in 0..=seg {
            let a = (a0 + 90.0 * i as f32 / seg as f32).to_radians();
            v.push(Vertex::new(cx + rad * a.cos(), cy + rad * a.sin(), 0.0, 0.0, 0.0, c));
        }
    }
    let n = (v.len() - 1) as u16;
    let mut idx = Vec::with_capacity(n as usize * 3);
    for i in 1..=n {
        idx.extend_from_slice(&[0, i, if i == n { 1 } else { i + 1 }]);
    }
    draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
}

pub fn gradient_rect(r: Rect, top: Color, bottom: Color) {
    let v = vec![
        Vertex::new(r.x, r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.x + r.w, r.y, 0.0, 0.0, 0.0, top),
        Vertex::new(r.x + r.w, r.y + r.h, 0.0, 0.0, 0.0, bottom),
        Vertex::new(r.x, r.y + r.h, 0.0, 0.0, 0.0, bottom),
    ];
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

pub fn star(cx: f32, cy: f32, r: f32, color: Color) {
    let pts: Vec<Vec2> = (0..10)
        .map(|i| {
            let a = (-90.0 + 36.0 * i as f32).to_radians();
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            vec2(cx + rr * a.cos(), cy + rr * a.sin())
        })
        .collect();
    let c = vec2(cx, cy);
    for i in 0..10 {
        draw_triangle(c, pts[i], pts[(i + 1) % 10], color);
    }
}

pub fn stars_row(cx: f32, cy: f32, r: f32, filled: u8) {
    let gap = r * 2.4;
    for i in 0..3u8 {
        let x = cx + (i as f32 - 1.0) * gap;
        star(x, cy, r, if i < filled { STAR_GOLD } else { alpha(MUTED, 0.35) });
    }
}

/// Draws a texture filling `r`, cropping it to keep the aspect ratio.
pub fn draw_cover(tex: &Texture2D, r: Rect) {
    let (tw, th) = (tex.width(), tex.height());
    let (ta, ra) = (tw / th, r.w / r.h);
    let src = if ta > ra {
        let sw = th * ra;
        Rect::new((tw - sw) / 2.0, 0.0, sw, th)
    } else {
        let sh = tw / ra;
        Rect::new(0.0, (th - sh) / 2.0, tw, sh)
    };
    draw_texture_ex(
        tex,
        r.x,
        r.y,
        WHITE,
        DrawTextureParams { dest_size: Some(vec2(r.w, r.h)), source: Some(src), ..Default::default() },
    );
}

pub fn icon_back(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    draw_line(c.x + s * 0.3, c.y - s, c.x - s * 0.5, c.y, 4.0, color);
    draw_line(c.x - s * 0.5, c.y, c.x + s * 0.3, c.y + s, 4.0, color);
}

pub fn icon_play(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    draw_triangle(
        vec2(c.x - s * 0.7, c.y - s),
        vec2(c.x - s * 0.7, c.y + s),
        vec2(c.x + s, c.y),
        color,
    );
}

pub fn icon_pause(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    draw_rectangle(c.x - s * 0.8, c.y - s, s * 0.55, s * 2.0, color);
    draw_rectangle(c.x + s * 0.25, c.y - s, s * 0.55, s * 2.0, color);
}

/// Back to the start: a bar followed by a left-pointing triangle (⏮).
pub fn icon_to_start(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    draw_rectangle(c.x - s * 0.85, c.y - s * 0.8, s * 0.3, s * 1.6, color);
    draw_triangle(
        vec2(c.x + s * 0.85, c.y - s * 0.8),
        vec2(c.x + s * 0.85, c.y + s * 0.8),
        vec2(c.x - s * 0.45, c.y),
        color,
    );
}

/// Repeat: two lines chasing each other around a loop (🔁).
pub fn icon_loop(c: Vec2, size: f32, color: Color) {
    let (w, h, t) = (size * 0.42, size * 0.26, size * 0.11);
    let head = size * 0.16;
    let line = |a: Vec2, b: Vec2| {
        draw_line(c.x + a.x, c.y + a.y, c.x + b.x, c.y + b.y, t, color);
        draw_circle(c.x + a.x, c.y + a.y, t / 2.0, color);
    };
    // top: left side up, then right towards the arrow
    line(vec2(-w, h * 0.25), vec2(-w, -h));
    line(vec2(-w, -h), vec2(w - head, -h));
    draw_triangle(
        vec2(c.x + w - head, c.y - h - head),
        vec2(c.x + w - head, c.y - h + head),
        vec2(c.x + w + head * 0.4, c.y - h),
        color,
    );
    // bottom: right side down, then left towards the arrow
    line(vec2(w, -h * 0.25), vec2(w, h));
    line(vec2(w, h), vec2(-w + head, h));
    draw_triangle(
        vec2(c.x - w + head, c.y + h - head),
        vec2(c.x - w + head, c.y + h + head),
        vec2(c.x - w - head * 0.4, c.y + h),
        color,
    );
}

pub fn icon_gear(c: Vec2, size: f32, color: Color, bg: Color) {
    let r = size * 0.32;
    for i in 0..8 {
        let a = (i as f32 * 45.0).to_radians();
        let d = vec2(a.cos(), a.sin());
        draw_line(c.x + d.x * r * 0.8, c.y + d.y * r * 0.8, c.x + d.x * r * 1.45, c.y + d.y * r * 1.45, size * 0.14, color);
    }
    draw_circle(c.x, c.y, r * 1.1, color);
    draw_circle(c.x, c.y, r * 0.5, bg);
}

/// Unified mouse/touch pointer.
///
/// Touches are read directly instead of through macroquad's mouse emulation: that
/// emulation ignores cancelled touches (the button then stays down forever) and
/// loses the press when a touch starts and ends within one frame.
#[derive(Clone, Copy, Default)]
pub struct Pointer {
    pub pos: Vec2,
    pub press_pos: Vec2,
    pub delta: Vec2,
    pub pressed: bool,
    pub down: bool,
    pub released: bool,
    /// Largest distance from the press position during this gesture.
    pub moved: f32,
    /// The finger currently acting as the pointer.
    touch_id: Option<u64>,
    /// The last input came from a mouse (so `pos` is a real hover position).
    mouse: bool,
    last_mouse: Vec2,
}

const TAP_SLOP: f32 = 18.0;

impl Pointer {
    pub fn update(&mut self) {
        let s = scale();
        let raw = Vec2::from(mouse_position());
        let mut pos = raw / s;
        let mut pressed = is_mouse_button_pressed(MouseButton::Left);
        let mut down = is_mouse_button_down(MouseButton::Left);
        let mut released = is_mouse_button_released(MouseButton::Left);

        let touches = touches();
        if !touches.is_empty() {
            self.mouse = false;
        } else if raw != self.last_mouse || pressed {
            self.mouse = true;
        }
        self.last_mouse = raw;

        for t in touches {
            let tpos = t.position / s;
            let ours = self.touch_id == Some(t.id);
            match t.phase {
                // A new finger takes over; this also recovers if the end of the
                // previous touch never arrived.
                TouchPhase::Started => {
                    self.touch_id = Some(t.id);
                    (pressed, down, pos) = (true, true, tpos);
                }
                TouchPhase::Moved | TouchPhase::Stationary if ours => {
                    (down, pos) = (true, tpos);
                }
                TouchPhase::Ended | TouchPhase::Cancelled if ours => {
                    self.touch_id = None;
                    (released, down, pos) = (true, false, tpos);
                }
                // Started and ended within one frame: only the end is left.
                TouchPhase::Ended | TouchPhase::Cancelled if self.touch_id.is_none() && !pressed => {
                    (pressed, released, down, pos) = (true, true, false, tpos);
                }
                _ => {}
            }
        }

        self.pressed = pressed;
        self.down = down;
        self.released = released;
        self.delta = if self.down && !self.pressed { pos - self.pos } else { Vec2::ZERO };
        if self.pressed {
            self.press_pos = pos;
            self.moved = 0.0;
        }
        if self.down || self.released {
            self.moved = self.moved.max((pos - self.press_pos).length());
        }
        self.pos = pos;
    }

    pub fn tapped(&self, r: Rect) -> bool {
        self.released && self.moved < TAP_SLOP && r.contains(self.pos) && r.contains(self.press_pos)
    }

    /// The mouse hovers over the circle. Never true for touch, which has no hover.
    pub fn hovering_circle(&self, c: Vec2, r: f32) -> bool {
        self.mouse && !self.dragging() && self.pos.distance(c) <= r
    }

    pub fn tapped_circle(&self, c: Vec2, r: f32) -> bool {
        self.released && self.moved < TAP_SLOP && self.pos.distance(c) <= r && self.press_pos.distance(c) <= r
    }

    pub fn pressing(&self, r: Rect) -> bool {
        self.down && self.moved < TAP_SLOP && r.contains(self.pos) && r.contains(self.press_pos)
    }

    pub fn dragging(&self) -> bool {
        self.down && self.moved >= TAP_SLOP
    }
}

pub fn button(f: &Fonts, p: &Pointer, r: Rect, label: &str, bg: Color, fg: Color, size: f32) -> bool {
    let c = if p.pressing(r) { mix(bg, WHITE, 0.15) } else { bg };
    rounded_rect(r, (r.h * 0.3).min(14.0), c);
    let size = fit_size(&f.bold, label, size, r.w - 16.0);
    text_center(&f.bold, label, r.x + r.w / 2.0, r.y + r.h / 2.0, size, fg);
    p.tapped(r)
}

/// Round icon button; `draw` paints the icon at the given center.
pub fn icon_button(p: &Pointer, r: Rect, bg: Color, draw: impl FnOnce(Vec2)) -> bool {
    let c = if p.pressing(r) { mix(bg, WHITE, 0.15) } else { bg };
    rounded_rect(r, r.h * 0.5, c);
    draw(r.center());
    p.tapped(r)
}

/// A row of options of which one is selected. Returns the tapped option.
pub fn segmented(f: &Fonts, p: &Pointer, r: Rect, labels: &[&str], selected: usize, accent: Color, size: f32) -> Option<usize> {
    rounded_rect(r, 12.0, PANEL);
    let w = r.w / labels.len() as f32;
    let mut out = None;
    for (i, l) in labels.iter().enumerate() {
        let cell = Rect::new(r.x + i as f32 * w, r.y, w, r.h);
        let inner = Rect::new(cell.x + 3.0, cell.y + 3.0, cell.w - 6.0, cell.h - 6.0);
        if i == selected {
            rounded_rect(inner, 10.0, accent);
        } else if p.pressing(cell) {
            rounded_rect(inner, 10.0, PANEL_HI);
        }
        let fg = if i == selected { BG } else { TEXT };
        let s = fit_size(&f.bold, l, size, inner.w - 10.0);
        text_center(&f.bold, l, cell.x + w / 2.0, cell.y + r.h / 2.0, s, fg);
        if p.tapped(cell) {
            out = Some(i);
        }
    }
    out
}
