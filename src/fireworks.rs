//! Celebration fireworks: rockets that rise and burst into glowing sparks.

use macroquad::prelude::*;
use macroquad::rand::gen_range;

/// How long new rockets are launched (seconds).
const SHOW_LEN: f32 = 4.5;
const ROCKETS: usize = 12;
const GRAVITY: f32 = 260.0;
/// Rockets decelerate faster than sparks fall, so they rise quickly and slow down before bursting.
const ROCKET_GRAVITY: f32 = 1300.0;

const COLORS: [Color; 7] = [
    Color::new(1.0, 0.30, 0.35, 1.0),
    Color::new(1.0, 0.77, 0.0, 1.0),
    Color::new(0.33, 0.88, 0.06, 1.0),
    Color::new(0.31, 0.76, 0.97, 1.0),
    Color::new(0.75, 0.45, 1.0, 1.0),
    Color::new(1.0, 0.55, 0.20, 1.0),
    Color::new(1.0, 1.0, 1.0, 1.0),
];

struct Rocket {
    pos: Vec2,
    vel: Vec2,
    /// Height at which it bursts.
    burst_y: f32,
    color: Color,
}

struct Spark {
    pos: Vec2,
    prev: Vec2,
    vel: Vec2,
    color: Color,
    age: f32,
    life: f32,
    size: f32,
    /// Glitter sparks flicker while they fade.
    glitter: bool,
}

struct Flash {
    pos: Vec2,
    color: Color,
    age: f32,
}

pub struct Fireworks {
    t: f32,
    /// Launch times of the rockets not yet fired, ascending.
    pending: Vec<f32>,
    rockets: Vec<Rocket>,
    sparks: Vec<Spark>,
    flashes: Vec<Flash>,
}

impl Fireworks {
    pub fn new() -> Fireworks {
        macroquad::rand::srand((get_time() * 1e6) as u64);
        // A quick opening salvo, then the rest spread over the show.
        let mut pending: Vec<f32> = (0..ROCKETS)
            .map(|i| if i < 3 { i as f32 * 0.15 } else { gen_range(0.4, SHOW_LEN) })
            .collect();
        pending.sort_by(|a, b| a.total_cmp(b));
        Fireworks { t: 0.0, pending, rockets: vec![], sparks: vec![], flashes: vec![] }
    }

    pub fn finished(&self) -> bool {
        self.pending.is_empty() && self.rockets.is_empty() && self.sparks.is_empty() && self.flashes.is_empty()
    }

    pub fn update(&mut self, dt: f32, area: Rect) {
        let dt = dt.min(0.05);
        self.t += dt;
        while self.pending.first().is_some_and(|&at| at <= self.t) {
            self.pending.remove(0);
            self.launch(area);
        }

        let mut bursts = vec![];
        self.rockets.retain_mut(|r| {
            r.vel.y += ROCKET_GRAVITY * dt;
            r.pos += r.vel * dt;
            if r.pos.y <= r.burst_y || r.vel.y >= 0.0 {
                bursts.push((r.pos, r.color));
                false
            } else {
                true
            }
        });
        // Trail behind the rising rockets
        for r in &self.rockets {
            self.sparks.push(Spark {
                pos: r.pos,
                prev: r.pos,
                vel: vec2(gen_range(-25.0, 25.0), gen_range(20.0, 70.0)),
                color: Color::new(1.0, 0.85, 0.55, 1.0),
                age: 0.0,
                life: gen_range(0.25, 0.5),
                size: 1.6,
                glitter: false,
            });
        }
        let scale = (area.h / 800.0).clamp(0.6, 1.5);
        for (pos, color) in bursts {
            self.burst(pos, color, scale);
        }

        for s in &mut self.sparks {
            s.age += dt;
            s.prev = s.pos;
            s.vel *= 1.0 - 1.6 * dt; // air drag
            s.vel.y += GRAVITY * dt;
            s.pos += s.vel * dt;
        }
        self.sparks.retain(|s| s.age < s.life);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < 0.35);
    }

    fn launch(&mut self, area: Rect) {
        let x = gen_range(area.x + area.w * 0.1, area.x + area.w * 0.9);
        let burst_y = gen_range(area.y + area.h * 0.12, area.y + area.h * 0.45);
        let rise = area.y + area.h - burst_y;
        // Fast enough to reach the burst height despite gravity
        let speed = (2.0 * ROCKET_GRAVITY * rise).sqrt() * 1.05;
        self.rockets.push(Rocket {
            pos: vec2(x, area.y + area.h),
            vel: vec2(gen_range(-40.0, 40.0), -speed),
            burst_y,
            color: COLORS[gen_range(0, COLORS.len())],
        });
    }

    fn burst(&mut self, pos: Vec2, color: Color, scale: f32) {
        self.flashes.push(Flash { pos, color, age: 0.0 });
        let second = COLORS[gen_range(0, COLORS.len())];
        let ring = gen_range(0, 3) == 0; // some bursts are a clean ring
        let n = gen_range(70, 110);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU + gen_range(-0.05, 0.05);
            let speed = if ring { 330.0 } else { gen_range(80.0, 360.0) } * scale;
            self.sparks.push(Spark {
                pos,
                prev: pos,
                vel: vec2(a.cos(), a.sin()) * speed,
                color: if gen_range(0, 4) == 0 { second } else { color },
                age: 0.0,
                life: gen_range(1.1, 1.9),
                size: gen_range(2.0, 3.4) * scale,
                glitter: gen_range(0, 3) == 0,
            });
        }
    }

    pub fn draw(&self) {
        for f in &self.flashes {
            let k = 1.0 - f.age / 0.35;
            draw_circle(f.pos.x, f.pos.y, 70.0 * (1.0 - k * 0.5), with_alpha(f.color, 0.25 * k));
            draw_circle(f.pos.x, f.pos.y, 18.0 * k, with_alpha(WHITE, 0.8 * k));
        }
        for r in &self.rockets {
            draw_circle(r.pos.x, r.pos.y, 6.0, with_alpha(r.color, 0.3));
            draw_circle(r.pos.x, r.pos.y, 2.6, WHITE);
        }
        for s in &self.sparks {
            let k = 1.0 - s.age / s.life;
            let mut a = k * k.sqrt();
            if s.glitter && k < 0.6 {
                a *= if (s.age * 30.0 + s.life * 100.0).sin() > 0.0 { 1.0 } else { 0.15 };
            }
            // Streak from the previous position, a soft glow and a hot core
            let tail = s.prev - (s.pos - s.prev) * 2.0;
            draw_line(tail.x, tail.y, s.pos.x, s.pos.y, s.size * 0.8, with_alpha(s.color, a * 0.5));
            draw_circle(s.pos.x, s.pos.y, s.size * 2.2, with_alpha(s.color, a * 0.22));
            draw_circle(s.pos.x, s.pos.y, s.size, with_alpha(mix_white(s.color, 0.4), a));
        }
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

fn mix_white(c: Color, k: f32) -> Color {
    Color::new(c.r + (1.0 - c.r) * k, c.g + (1.0 - c.g) * k, c.b + (1.0 - c.b) * k, c.a)
}
