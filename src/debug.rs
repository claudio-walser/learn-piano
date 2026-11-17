//! Diagnostics for troubleshooting on the target device.
//!
//! Enabled with `--debug` or `OPENETUDE_DEBUG=1`. Shows FPS, frame times and the
//! raw mouse/touch input in a corner, and writes the same to
//! `~/.local/share/openetude/debug.log` (and stderr).

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;

use macroquad::prelude::*;

use crate::ui::{scale, text_left, vh, vw, Fonts};

pub struct Debug {
    log: Option<File>,
    frame: u64,
    /// Recent frame times in seconds.
    times: VecDeque<f32>,
    lines: VecDeque<String>,
    last_screen: String,
}

impl Debug {
    pub fn from_env() -> Option<Debug> {
        let on = std::env::args().any(|a| a == "--debug")
            || std::env::var("OPENETUDE_DEBUG").is_ok_and(|v| v != "0" && !v.is_empty());
        if !on {
            return None;
        }
        let log = crate::progress::data_dir().and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            File::create(dir.join("debug.log")).ok()
        });
        let mut d = Debug { log, frame: 0, times: VecDeque::new(), lines: VecDeque::new(), last_screen: String::new() };
        let info = unsafe { get_internal_gl() }.quad_context.info();
        d.write(format!(
            "start: backend {:?}, gl {:?}, window {}x{}, dpi scale {}, ui scale {:.3}",
            info.backend,
            info.gl_version_string,
            screen_width(),
            screen_height(),
            screen_dpi_scale(),
            scale()
        ));
        Some(d)
    }

    fn write(&mut self, line: String) {
        let line = format!("[{:>6} {:>8.3}s] {line}", self.frame, get_time());
        eprintln!("{line}");
        if let Some(f) = &mut self.log {
            let _ = writeln!(f, "{line}");
        }
        self.lines.push_back(line);
        while self.lines.len() > 12 {
            self.lines.pop_front();
        }
    }

    /// Call once per frame, right after the pointer update.
    pub fn input(&mut self, screen: &str) {
        self.frame += 1;
        let dt = get_frame_time();
        self.times.push_back(dt);
        while self.times.len() > 120 {
            self.times.pop_front();
        }
        if screen != self.last_screen {
            self.last_screen = screen.to_string();
            self.write(format!("screen: {screen}"));
        }
        if dt > 0.05 {
            self.write(format!("slow frame: {:.0} ms", dt * 1000.0));
        }
        let (mx, my) = mouse_position();
        for (name, b) in [("left", MouseButton::Left), ("right", MouseButton::Right), ("middle", MouseButton::Middle)] {
            let (p, r, d) = (is_mouse_button_pressed(b), is_mouse_button_released(b), is_mouse_button_down(b));
            if p || r {
                self.write(format!("mouse {name}: pressed={p} released={r} down={d} at ({mx:.0}, {my:.0})"));
            }
        }
        for t in touches() {
            if t.phase != TouchPhase::Stationary && t.phase != TouchPhase::Moved {
                self.write(format!("touch {}: {:?} at ({:.0}, {:.0})", t.id, t.phase, t.position.x, t.position.y));
            }
        }
    }

    pub fn draw(&self, fonts: &Fonts) {
        let n = self.times.len().max(1) as f32;
        let avg = self.times.iter().sum::<f32>() / n;
        let worst = self.times.iter().copied().fold(0.0f32, f32::max);
        let (mx, my) = mouse_position();
        let mut rows = vec![format!(
            "{:.0} fps · avg {:.1} ms · worst {:.1} ms · frame {} · mouse ({mx:.0}, {my:.0}) {}",
            1.0 / avg.max(1e-6),
            avg * 1000.0,
            worst * 1000.0,
            self.frame,
            if is_mouse_button_down(MouseButton::Left) { "DOWN" } else { "up" }
        )];
        rows.extend(self.lines.iter().rev().take(6).cloned());
        let w = 620.0f32.min(vw() - 16.0);
        let h = 10.0 + rows.len() as f32 * 20.0;
        // Middle of the screen: keeps the buttons at the top and the keys visible
        let y0 = vh() * 0.4;
        let x0 = 8.0;
        draw_rectangle(x0, y0, w, h, Color::new(0.0, 0.0, 0.0, 0.75));
        for (i, r) in rows.iter().enumerate() {
            text_left(&fonts.regular, r, x0 + 8.0, y0 + 15.0 + i as f32 * 20.0, 13.0, if i == 0 { YELLOW } else { WHITE });
        }
    }
}
