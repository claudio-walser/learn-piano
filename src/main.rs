mod audio;
mod debug;
mod fireworks;
mod i18n;
mod keyboard;
mod library;
mod midi;
mod practice;
mod progress;
mod score;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use macroquad::prelude::*;

use audio::{Audio, Status};
use fireworks::Fireworks;
use i18n::{Lang, Strings};
use keyboard::{is_black, KeyLayout, KeyboardSize};
use library::{Lesson, Library};
use midi::{Midi, MidiEvent};
use practice::{Hand, Hands, Judge, Mode, Players, Practice, Summary};
use progress::Progress;
use ui::*;

/// Score seconds visible above the keyboard.
const LOOKAHEAD: f64 = 3.0;
const TEMPOS: [f64; 3] = [0.5, 0.75, 1.0];

fn window_conf() -> Conf {
    Conf {
        window_title: "Learn Piano".to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        fullscreen: std::env::args().any(|a| a == "--fullscreen"),
        // 0, not 1: EGL treats this as a minimum, and GPUs without MSAA (old Intel) then
        // offer no config at all. GLX treats 0 and 1 the same.
        sample_count: 0,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            // Under XWayland, touch arrives only as emulated mouse events, which do not
            // reach the app on every system. Native Wayland delivers real touch events.
            linux_backend: if use_wayland() {
                miniquad::conf::LinuxBackend::WaylandWithX11Fallback
            } else {
                miniquad::conf::LinuxBackend::X11Only
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Native Wayland in a Wayland session, X11 otherwise.
///
/// Under XWayland, touches never send a button release, so taps don't work.
/// `--x11` / `--wayland` (or `LEARN_PIANO_BACKEND=x11|wayland`) override the choice.
fn use_wayland() -> bool {
    let arg = |a: &str| std::env::args().any(|x| x == a);
    let env = std::env::var("LEARN_PIANO_BACKEND").unwrap_or_default();
    if arg("--x11") || env == "x11" {
        return false;
    }
    if arg("--wayland") || env == "wayland" {
        return true;
    }
    // The screenshot tour reads the framebuffer back, which only works on X11.
    if std::env::var_os("LEARN_PIANO_SHOTS").is_some() {
        return false;
    }
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

#[macroquad::main(window_conf)]
async fn main() {
    // Pointer handles touches itself (see ui::Pointer).
    simulate_mouse_with_touch(false);
    let mut app = App::new();
    loop {
        app.frame();
        next_frame().await;
    }
}

enum Screen {
    Dashboard,
    Difficulty(usize),
    Player(Box<Player>),
    Error(String),
}

struct Player {
    comp: usize,
    level: usize,
    lesson: usize,
    practice: Practice,
    range: (u8, u8),
    result: Option<Summary>,
    /// Result of the previous run while repeating, with the time it ended.
    last_run: Option<(Summary, f64)>,
    /// Celebration for a run above 90 %.
    fireworks: Option<Fireworks>,
    touch_key: Option<u8>,
}

enum Action {
    Home,
    OpenComposition(usize),
    StartLevel(usize, usize),
    OpenLesson(usize, usize, usize),
}

struct App {
    fonts: Fonts,
    lib: Option<Library>,
    thumbs: Vec<Option<Texture2D>>,
    audio: Audio,
    midi: Midi,
    echo: Arc<AtomicBool>,
    progress: Progress,
    lang: Lang,
    screen: Screen,
    pointer: Pointer,
    /// Dev aid: `LEARN_PIANO_SHOTS=<dir>` walks through all screens, saves PNGs and quits.
    shots: Option<(std::path::PathBuf, u32)>,
    scroll: f32,
    show_settings: bool,
    /// `--debug`: FPS and raw input overlay plus log file.
    debug: Option<debug::Debug>,
}

fn lesson_key(lib: &Library, c: usize, l: usize, lesson: &Lesson) -> String {
    let comp = &lib.compositions[c];
    format!("{}/{}/{}", comp.id, comp.levels[l].difficulty, lesson.stem)
}

fn lesson_label(s: &Strings, lesson: &Lesson) -> String {
    if lesson.is_full {
        s.full_piece.to_string()
    } else {
        format!("{} {}", s.part, lesson.label)
    }
}

fn hand_color(h: Hand) -> Color {
    match h {
        Hand::Right => RIGHT_HAND,
        Hand::Left => LEFT_HAND,
    }
}

impl App {
    fn new() -> App {
        let progress = Progress::load();
        let lang = progress.settings.language.unwrap_or_else(Lang::detect);
        let (lib, screen) = match library::find_root() {
            Some(root) => match Library::load(&root) {
                Ok(l) => (Some(l), Screen::Dashboard),
                Err(e) => (None, Screen::Error(e)),
            },
            None => (None, Screen::Error("compositions.xml not found (use --library <dir>)".into())),
        };
        let thumbs = lib
            .as_ref()
            .map(|l| {
                l.compositions
                    .iter()
                    .map(|c| {
                        let bytes = std::fs::read(c.thumbnail.as_ref()?).ok()?;
                        let tex = Texture2D::from_file_with_format(&bytes, None);
                        tex.set_filter(FilterMode::Linear);
                        Some(tex)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let audio = Audio::start(lib.as_ref().map(|l| l.root.join("sound-fonts").join("default.sf2")));
        let echo = Arc::new(AtomicBool::new(progress.settings.echo_input));
        let midi = Midi::new(audio.sender(), echo.clone());
        if progress.settings.fullscreen {
            set_fullscreen(true);
        }
        App {
            fonts: Fonts::load(),
            lib,
            thumbs,
            audio,
            midi,
            echo,
            progress,
            lang,
            screen,
            pointer: Pointer::default(),
            scroll: 0.0,
            show_settings: false,
            shots: std::env::var_os("LEARN_PIANO_SHOTS").map(|d| (d.into(), 0)),
            debug: debug::Debug::from_env(),
        }
    }

    fn s(&self) -> &'static Strings {
        self.lang.strings()
    }

    fn frame(&mut self) {
        begin_frame();
        self.pointer.update();
        if let Some(d) = &mut self.debug {
            let name = match &self.screen {
                Screen::Dashboard => "dashboard",
                Screen::Difficulty(_) => "difficulty",
                Screen::Player(_) => "player",
                Screen::Error(_) => "error",
            };
            d.input(name);
        }
        self.midi.poll(get_time());
        let events: Vec<MidiEvent> = self.midi.rx.try_iter().collect();
        clear_background(BG);

        // A modal only reacts to input from the frame after it opened; otherwise the
        // release that opened it would count as a tap outside and close it again.
        let settings_was_open = self.show_settings;
        let ptr = if self.show_settings { Pointer::default() } else { self.pointer };
        let mut screen = std::mem::replace(&mut self.screen, Screen::Dashboard);
        let mut action = match &mut screen {
            Screen::Dashboard => self.dashboard(&ptr),
            Screen::Difficulty(c) => self.difficulty(*c, &ptr),
            Screen::Player(p) => self.player(p, &ptr, &events),
            Screen::Error(e) => self.error(e, &ptr),
        };
        self.screen = screen;

        if self.show_settings {
            let ptr = if settings_was_open { self.pointer } else { Pointer::default() };
            self.settings(ptr);
        }
        if is_key_pressed(KeyCode::Escape) {
            if self.show_settings {
                self.show_settings = false;
            } else {
                action = match &self.screen {
                    Screen::Difficulty(_) | Screen::Error(_) => Some(Action::Home),
                    Screen::Player(p) => Some(Action::OpenComposition(p.comp)),
                    Screen::Dashboard => None,
                };
            }
        }
        if let Some(a) = action {
            self.apply(a);
        }
        if let Some(d) = &self.debug {
            d.draw(&self.fonts);
        }
        self.screenshot_tour();
    }

    fn screenshot_tour(&mut self) {
        let Some((dir, frame)) = self.shots.as_mut() else { return };
        *frame += 1;
        let (frame, dir) = (*frame, dir.clone());
        let shot = |name: &str| get_screen_data().export_png(&dir.join(name).to_string_lossy());
        match frame {
            20 => {
                shot("1-dashboard.png")
            }
            21 => self.apply(Action::OpenComposition(0)),
            40 => shot("2-difficulty.png"),
            41 => self.apply(Action::StartLevel(0, 0)),
            // lead-in is 2.5 s; at ~60 fps the first chord waits from ~frame 200
            240 => shot("3-player-wait.png"),
            241 => {
                if let Screen::Player(p) = &mut self.screen {
                    p.practice.configure(Mode::Realtime, Hands::Right, p.practice.players, &self.audio);
                    p.practice.t = 3.0;
                }
            }
            260 => shot("4-player-realtime.png"),
            261 => {
                if let Screen::Player(p) = &mut self.screen {
                    p.result = Some(p.practice.summary());
                }
            }
            280 => shot("5-result.png"),
            281 => {
                self.apply(Action::Home);
                self.show_settings = true;
            }
            300 => shot("6-settings.png"),
            301 => std::process::exit(0),
            _ => {}
        }
    }

    fn apply(&mut self, a: Action) {
        self.audio.all_off();
        match a {
            Action::Home => self.screen = Screen::Dashboard,
            Action::OpenComposition(c) => self.screen = Screen::Difficulty(c),
            Action::StartLevel(c, l) => {
                let Some(lib) = &self.lib else { return };
                let lessons = &lib.compositions[c].levels[l].lessons;
                // Continue with the first lesson not passed yet.
                let i = lessons
                    .iter()
                    .position(|ls| self.progress.stars(&lesson_key(lib, c, l, ls)) == 0)
                    .unwrap_or(lessons.len() - 1);
                self.open_lesson(c, l, i);
            }
            Action::OpenLesson(c, l, i) => self.open_lesson(c, l, i),
        }
    }

    fn open_lesson(&mut self, c: usize, l: usize, i: usize) {
        let Some(lib) = &self.lib else { return };
        let level = &lib.compositions[c].levels[l];
        let lesson = &level.lessons[i];
        let score = match score::load(&lesson.path) {
            Ok(s) => s,
            Err(e) => {
                self.screen = Screen::Error(e);
                return;
            }
        };
        // Keep the keyboard identical across all lessons of a level.
        let mut pitches: Vec<u8> = score.notes.iter().map(|n| n.pitch).collect();
        if let Some(full) = level.lessons.last().filter(|_| !lesson.is_full).and_then(|f| score::load(&f.path).ok()) {
            pitches.extend(full.notes.iter().map(|n| n.pitch));
        }
        let st = &self.progress.settings;
        let range = keyboard::range_for(st.keyboard, pitches.into_iter());
        let practice = Practice::new(score, st.mode, st.hands, st.players, st.tempo);
        self.screen = Screen::Player(Box::new(Player {
            comp: c,
            level: l,
            lesson: i,
            practice,
            range,
            result: None,
            last_run: None,
            fireworks: None,
            touch_key: None,
        }));
    }

    // ---------------------------------------------------------------- header

    fn midi_pill(&self, rx: f32, cy: f32) {
        let f = &self.fonts;
        let (label, color) = match &self.midi.port_name {
            Some(n) => (n.split(':').next().unwrap_or(n).trim().to_string(), GOOD),
            None => ("MIDI –".to_string(), MUTED),
        };
        let size = 17.0;
        let w = text_width(&f.regular, &label, size).min(260.0) + 40.0;
        let r = Rect::new(rx - w, cy - 18.0, w, 36.0);
        rounded_rect(r, 18.0, PANEL);
        draw_circle(r.x + 18.0, cy, 6.0, color);
        let size = fit_size(&f.regular, &label, size, w - 40.0);
        text_left(&f.regular, &label, r.x + 32.0, cy, size, TEXT);
    }

    // ------------------------------------------------------------- dashboard

    fn dashboard(&mut self, ptr: &Pointer) -> Option<Action> {
        let s = self.s();
        let (w, h) = (vw(), vh());
        let header = 96.0;
        let Some(lib) = &self.lib else { return None };
        let lang = self.lang.code();

        let gap = 28.0;
        let cw = (w - 48.0).min(280.0);
        let ch = cw + 104.0;
        let cols = ((w - 48.0 + gap) / (cw + gap)).floor().max(1.0) as usize;
        let rows = lib.compositions.len().div_ceil(cols);
        let content_h = rows as f32 * (ch + gap) + 24.0;
        let max_scroll = (content_h - (h - header)).max(0.0);
        if ptr.dragging() {
            self.scroll -= ptr.delta.y;
        }
        self.scroll = (self.scroll - wheel_notches() * 60.0).clamp(0.0, max_scroll);

        let grid_w = cols as f32 * cw + (cols - 1) as f32 * gap;
        let x0 = (w - grid_w) / 2.0;
        let y0 = header + 12.0 - self.scroll;
        let mut action = None;
        for (i, comp) in lib.compositions.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let card = Rect::new(x0 + col as f32 * (cw + gap), y0 + row as f32 * (ch + gap), cw, ch);
            if card.y > h || card.y + card.h < header {
                continue;
            }
            rounded_rect(card, 18.0, if ptr.pressing(card) { PANEL_HI } else { PANEL });
            let img = Rect::new(card.x + 12.0, card.y + 12.0, cw - 24.0, cw - 24.0);
            match self.thumbs.get(i).and_then(|t| t.as_ref()) {
                Some(t) => draw_cover(t, img),
                None => rounded_rect(img, 12.0, PANEL_HI),
            }
            let title = comp.title.get(lang);
            let ts = fit_size(&self.fonts.bold, title, 23.0, cw - 32.0);
            text_left(&self.fonts.bold, title, card.x + 16.0, img.y + img.h + 26.0, ts, TEXT);
            if let Some(c) = &comp.composer {
                text_left(&self.fonts.regular, c, card.x + 16.0, img.y + img.h + 54.0, 17.0, MUTED);
            }
            // One dot per difficulty level; filled when the full piece was passed.
            for (li, level) in comp.levels.iter().enumerate() {
                let full = level.lessons.last().unwrap();
                let done = self.progress.stars(&lesson_key(lib, i, li, full)) > 0;
                let col = difficulty_color(level.difficulty_index);
                let (cx, cy) = (card.x + 24.0 + li as f32 * 26.0, card.y + card.h - 20.0);
                if done {
                    draw_circle(cx, cy, 8.0, col);
                } else {
                    draw_circle_lines(cx, cy, 8.0, 2.0, col);
                }
            }
            if ptr.tapped(card) && ptr.pos.y > header {
                action = Some(Action::OpenComposition(i));
            }
        }
        if lib.compositions.is_empty() {
            text_center(&self.fonts.regular, s.no_compositions, w / 2.0, h / 2.0, 24.0, MUTED);
        }

        draw_rectangle(0.0, 0.0, w, header, BG);
        text_left(&self.fonts.bold, s.app_title, 32.0, header / 2.0, 38.0, TEXT);
        let gear = Rect::new(w - 76.0, header / 2.0 - 26.0, 52.0, 52.0);
        if icon_button(ptr, gear, PANEL, |c| icon_gear(c, 40.0, TEXT, PANEL)) {
            self.show_settings = true;
        }
        self.midi_pill(gear.x - 14.0, header / 2.0);
        action
    }

    // ------------------------------------------------------------ difficulty

    fn difficulty(&mut self, c: usize, ptr: &Pointer) -> Option<Action> {
        let s = self.s();
        let lang = self.lang.code();
        let (w, h) = (vw(), vh());
        let lib = self.lib.as_ref()?;
        let comp = &lib.compositions[c];
        let f = &self.fonts;
        let sc = (h / 800.0).min(w / 900.0).clamp(0.5, 1.6);

        let hero_h = h * 0.38;
        gradient_rect(Rect::new(0.0, 0.0, w, hero_h), rgb(0x3d322b), rgb(0x27221f));
        let side = hero_h * 0.82;
        let img = Rect::new((w - side) / 2.0, (hero_h - side) / 2.0, side, side);
        match self.thumbs.get(c).and_then(|t| t.as_ref()) {
            Some(t) => draw_cover(t, img),
            None => rounded_rect(img, 16.0, PANEL),
        }
        let mut action = None;
        let back = Rect::new(20.0, 20.0, 56.0, 56.0);
        if icon_button(ptr, back, alpha(BLACK, 0.35), |p| icon_back(p, 28.0, TEXT)) {
            action = Some(Action::Home);
        }

        let mut y = hero_h + 62.0 * sc;
        let title = comp.title.get(lang);
        text_center(&f.bold, title, w / 2.0, y, fit_size(&f.bold, title, 56.0 * sc, w - 40.0), TEXT);
        if let Some(composer) = &comp.composer {
            y += 58.0 * sc;
            text_center(&f.regular, composer, w / 2.0, y, 34.0 * sc, MUTED);
        }
        y += 72.0 * sc;
        text_center(&f.regular, s.choose_difficulty, w / 2.0, y, 30.0 * sc, TEXT);

        // Duets sit apart from the difficulty steps: half a slot of extra space.
        let gap_before = |li: usize| {
            let l = &comp.levels;
            if li > 0 && l[li].is_duet && !l[li - 1].is_duet { 0.5 } else { 0.0 }
        };
        let slot: Vec<f32> = (0..comp.levels.len())
            .scan(0.0, |acc, li| {
                *acc += gap_before(li);
                Some(li as f32 + *acc)
            })
            .collect();
        let n = slot.last().map_or(1.0, |&s| s + 1.0);
        let below = 92.0 * sc; // room for name, stars and lesson count
        let avail_h = h - (y + 40.0 * sc) - below;
        let r = (96.0 * sc).min(w / (n * 2.5)).min(avail_h / 2.0).max(30.0);
        let cy = y + 36.0 * sc + r;
        let spacing = r * 2.3;
        let recommended = comp.levels.iter().enumerate().position(|(li, lv)| {
            !lv.is_duet && self.progress.stars(&lesson_key(lib, c, li, lv.lessons.last().unwrap())) == 0
        });

        for (li, level) in comp.levels.iter().enumerate() {
            let cx = w / 2.0 + (slot[li] - (n - 1.0) / 2.0) * spacing;
            let col = difficulty_color(level.difficulty_index);
            let name = lib.difficulty_title(level, lang);
            let hot = ptr.down && ptr.pos.distance(vec2(cx, cy)) <= r && ptr.moved < 18.0;
            // Name on every button; the play icon only while the mouse is over it
            // (touch has no hover). The recommended level is filled.
            if hot || ptr.hovering_circle(vec2(cx, cy), r) {
                draw_poly(cx, cy, 64, r, 0.0, if hot { mix(col, WHITE, 0.2) } else { col });
                icon_play(vec2(cx, cy), r * 0.62, alpha(BG, 0.92));
            } else {
                let fg = if Some(li) == recommended {
                    draw_poly(cx, cy, 64, r, 0.0, col);
                    BG
                } else {
                    draw_poly_lines(cx, cy, 64, r, 0.0, 2.5, col);
                    col
                };
                let fs = fit_size(&f.bold, &name, 26.0 * sc, r * 1.8);
                text_center(&f.bold, &name, cx, cy, fs, fg);
            }
            let full = level.lessons.last().unwrap();
            let stars = self.progress.stars(&lesson_key(lib, c, li, full));
            let ly = cy + r + 22.0 * sc;
            stars_row(cx, ly, 10.0 * sc, stars);
            if level.lessons.len() > 1 {
                let txt = format!("{} {}", level.lessons.len(), s.lessons);
                text_center(&f.regular, &txt, cx, ly + 28.0 * sc, 18.0 * sc, MUTED);
            }
            if ptr.tapped_circle(vec2(cx, cy), r) {
                action = Some(Action::StartLevel(c, li));
            }
        }
        action
    }

    // ---------------------------------------------------------------- player

    fn player(&mut self, p: &mut Player, ptr: &Pointer, events: &[MidiEvent]) -> Option<Action> {
        let s = self.s();
        let lang = self.lang.code();
        let (w, h) = (vw(), vh());
        let lib = self.lib.as_ref()?;
        let comp = &lib.compositions[p.comp];
        let level = &comp.levels[p.level];
        let mut action = None;

        // Input
        let modal = p.result.is_some();
        for ev in events {
            match *ev {
                MidiEvent::On(k) if !modal => p.practice.key_down(k),
                MidiEvent::On(_) => {}
                MidiEvent::Off(k) => p.practice.key_up(k),
            }
        }

        // Layout
        let top_h = 68.0;
        let ctrl_h = 64.0;
        let kb_h = (h * 0.24).clamp(100.0, 230.0);
        let kb = Rect::new(0.0, h - kb_h, w, kb_h);
        let wf = Rect::new(0.0, top_h + ctrl_h, w, kb.y - top_h - ctrl_h);
        let keys = KeyLayout::new(p.range.0, p.range.1, kb);

        // On-screen keyboard
        if !modal && ptr.pressed {
            if let Some(k) = keys.hit(ptr.pos) {
                p.touch_key = Some(k);
                self.audio.note_on(0, k, 90);
                p.practice.key_down(k);
            }
        }
        if let Some(k) = p.touch_key {
            if !ptr.down {
                self.audio.note_off(0, k);
                p.practice.key_up(k);
                p.touch_key = None;
            }
        }
        if !modal && ptr.tapped(wf) {
            let paused = !p.practice.paused;
            p.practice.set_paused(paused, &self.audio);
        }
        // Seeking: wheel up / drag down moves the notes down, i.e. forward in time.
        // It pauses, so the spot can be looked at; a tap plays on from there.
        if !modal {
            let pps = wf.h as f64 / LOOKAHEAD;
            let mut dt = wheel_notches() as f64 * 60.0 / pps;
            if ptr.dragging() && wf.contains(ptr.press_pos) {
                dt += ptr.delta.y as f64 / pps;
            }
            if dt != 0.0 {
                p.practice.set_paused(true, &self.audio);
                let t = p.practice.t + dt;
                p.practice.seek(t, &self.audio);
                p.last_run = None;
            }
        }

        p.practice.update(get_frame_time().min(0.1), &self.audio);
        if p.practice.finished && p.result.is_none() {
            let sum = p.practice.summary();
            // A run with seeking is no real attempt: no stars, no fireworks.
            let counts = p.practice.mode != Mode::Listen && !p.practice.seeked;
            if counts {
                let key = lesson_key(lib, p.comp, p.level, &level.lessons[p.lesson]);
                self.progress.record(key, sum.stars, sum.percent);
            }
            if counts && sum.percent > 90.0 {
                p.fireworks = Some(Fireworks::new());
            }
            if self.progress.settings.repeat {
                p.practice.restart(&self.audio);
                p.last_run = Some((sum, get_time()));
            } else {
                p.result = Some(sum);
            }
        }

        self.draw_waterfall(&p.practice, &keys, wf);
        self.draw_keyboard(&p.practice, &keys);
        if let Some(fx) = &mut p.fireworks {
            fx.update(get_frame_time(), Rect::new(0.0, 0.0, w, h));
            if fx.finished() {
                p.fireworks = None;
            }
        }
        // Without the result overlay the fireworks go over the waterfall.
        if let Some(fx) = p.fireworks.as_ref().filter(|_| p.result.is_none()) {
            fx.draw();
        }

        // Status texts in the waterfall
        let f = &self.fonts;
        let pill = |txt: &str, color: Color, right: bool| {
            let tw = text_width(&f.regular, txt, 16.0) + 28.0;
            let x = if right { w - 16.0 - tw } else { 16.0 };
            let r = Rect::new(x, wf.y + 14.0, tw, 32.0);
            rounded_rect(r, 16.0, alpha(BG, 0.85));
            text_left(&f.regular, txt, r.x + 14.0, r.y + 16.0, 16.0, color);
        };
        match self.audio.status() {
            Status::Loading => pill(s.loading_sound, MUTED, false),
            Status::Failed(_) => pill(s.sound_failed, BAD, false),
            Status::Ready => {}
        }
        if self.midi.port_name.is_none() {
            pill(s.no_midi, MUTED, true);
        }
        if let Some((sum, at)) = p.last_run {
            if get_time() - at < 5.0 {
                // Result of the previous run, centered at the top of the waterfall
                let txt = format!("{:.0} %", sum.percent);
                let tw = text_width(&f.bold, &txt, 18.0);
                let r = Rect::new(w / 2.0 - (tw + 140.0) / 2.0, wf.y + 14.0, tw + 140.0, 36.0);
                rounded_rect(r, 18.0, alpha(BG, 0.85));
                stars_row(r.x + 60.0, r.y + 18.0, 9.0, sum.stars);
                text_left(&f.bold, &txt, r.x + 116.0, r.y + 18.0, 18.0, TEXT);
            }
        }
        if p.practice.paused && p.practice.seeked {
            // Seeking: keep the notes visible, only a small hint.
            let tw = text_width(&f.regular, s.paused, 16.0) + 28.0;
            let r = Rect::new(w / 2.0 - tw / 2.0, wf.y + wf.h - 52.0, tw, 32.0);
            rounded_rect(r, 16.0, alpha(BG, 0.85));
            text_center(&f.regular, s.paused, r.x + r.w / 2.0, r.y + 16.0, 16.0, TEXT);
        } else if p.practice.paused {
            draw_rectangle(wf.x, wf.y, wf.w, wf.h, alpha(BLACK, 0.45));
            let c = wf.center();
            draw_circle(c.x, c.y, 54.0, alpha(WHITE, 0.15));
            icon_play(c, 50.0, TEXT);
            text_center(&f.regular, s.paused, c.x, c.y + 88.0, 22.0, TEXT);
        }

        // Top bar
        draw_rectangle(0.0, 0.0, w, top_h + ctrl_h, BG);
        let back = Rect::new(12.0, 8.0, 52.0, 52.0);
        if icon_button(ptr, back, PANEL, |c| icon_back(c, 26.0, TEXT)) {
            action = Some(Action::OpenComposition(p.comp));
        }
        let diff_col = difficulty_color(level.difficulty_index);

        // Lesson chips (right aligned)
        let mut chips_left = w - 16.0;
        if level.lessons.len() > 1 {
            let x = &mut chips_left;
            for (i, ls) in level.lessons.iter().enumerate().rev() {
                let label = lesson_label(s, ls);
                let done = self.progress.stars(&lesson_key(lib, p.comp, p.level, ls)) > 0;
                let cw = text_width(&f.bold, &label, 17.0) + if done { 50.0 } else { 30.0 };
                *x -= cw;
                let r = Rect::new(*x, 14.0, cw, 40.0);
                let selected = i == p.lesson;
                rounded_rect(r, 20.0, if selected { diff_col } else if ptr.pressing(r) { PANEL_HI } else { PANEL });
                let fg = if selected { BG } else { TEXT };
                if done {
                    star(r.x + 20.0, r.y + 20.0, 9.0, if selected { BG } else { STAR_GOLD });
                    text_left(&f.bold, &label, r.x + 34.0, r.y + 20.0, 17.0, fg);
                } else {
                    text_left(&f.bold, &label, r.x + 15.0, r.y + 20.0, 17.0, fg);
                }
                if ptr.tapped(r) && !selected {
                    action = Some(Action::OpenLesson(p.comp, p.level, i));
                }
                *x -= 8.0;
            }
        }

        // Titles, shrunk to fit left of the chips
        let title_w = chips_left - 80.0 - 12.0;
        let title = comp.title.get(lang);
        text_left(&f.bold, title, 80.0, 24.0, fit_size(&f.bold, title, 24.0, title_w), TEXT);
        let lesson_title = p
            .practice
            .score
            .title
            .clone()
            .unwrap_or_else(|| lesson_label(s, &level.lessons[p.lesson]));
        let sub = format!("{} · {}", lib.difficulty_title(level, lang), lesson_title);
        text_left(&f.regular, &sub, 80.0, 50.0, fit_size(&f.regular, &sub, 17.0, title_w), diff_col);

        // Control row
        // Duets get a fourth control: who plays which part.
        let duet = p.practice.score.is_duet();
        let players_w = if duet { 380.0 + 16.0 } else { 0.0 };
        let total = 380.0 + players_w + 320.0 + 250.0 + 3.0 * 52.0 + 5.0 * 16.0;
        let k = ((w - 32.0) / total).min(1.0);
        let cy = top_h + 6.0;
        let mut x = 16.0;
        let mut seg = |width: f32| {
            let r = Rect::new(x, cy, width * k, 50.0);
            x += width * k + 16.0 * k;
            r
        };
        let r_mode = seg(380.0);
        let r_players = if duet { Some(seg(380.0)) } else { None };
        let r_hands = seg(320.0);
        let r_tempo = seg(250.0);
        let fs = 18.0 * k.max(0.8);
        let modes = [Mode::Wait, Mode::Realtime, Mode::Listen];
        let mi = modes.iter().position(|&m| m == p.practice.mode).unwrap_or(0);
        let mut changed = false;
        if let Some(i) = segmented(f, ptr, r_mode, &[s.mode_wait, s.mode_realtime, s.mode_listen], mi, TEXT, fs) {
            self.progress.settings.mode = modes[i];
            changed = true;
        }
        if let Some(r) = r_players {
            let players = [Players::Student, Players::Teacher, Players::Together];
            let pi = players.iter().position(|&x| x == p.practice.players).unwrap_or(0);
            if let Some(i) = segmented(f, ptr, r, &s.players, pi, TEXT, fs) {
                self.progress.settings.players = players[i];
                changed = true;
            }
        }
        let hands = [Hands::Left, Hands::Right, Hands::Both];
        let hi = hands.iter().position(|&x| x == p.practice.hands).unwrap_or(2);
        let accent = match p.practice.hands {
            Hands::Left => LEFT_HAND,
            Hands::Right => RIGHT_HAND,
            Hands::Both => TEXT,
        };
        if let Some(i) = segmented(f, ptr, r_hands, &[s.hand_left, s.hand_right, s.hand_both], hi, accent, fs) {
            self.progress.settings.hands = hands[i];
            changed = true;
        }
        let ti = TEMPOS.iter().position(|&t| (t - p.practice.tempo).abs() < 0.01).unwrap_or(2);
        if let Some(i) = segmented(f, ptr, r_tempo, &["50 %", "75 %", "100 %"], ti, TEXT, fs) {
            self.progress.settings.tempo = TEMPOS[i];
            p.practice.tempo = TEMPOS[i];
            self.progress.save();
        }
        if changed {
            let st = &self.progress.settings;
            p.practice.configure(st.mode, st.hands, st.players, &self.audio);
            p.result = None;
            p.last_run = None;
            self.progress.save();
        }
        let b = 52.0;
        let r_pause = Rect::new(w - 16.0 - b, cy - 1.0, b, b);
        let r_restart = Rect::new(r_pause.x - 12.0 - b, cy - 1.0, b, b);
        let r_repeat = Rect::new(r_restart.x - 12.0 - b, cy - 1.0, b, b);
        if r_repeat.x > x - 8.0 {
            let repeat = self.progress.settings.repeat;
            let (bg, fg) = if repeat { (TEXT, BG) } else { (PANEL, TEXT) };
            if icon_button(ptr, r_repeat, bg, |c| icon_loop(c, 30.0, fg)) {
                self.progress.settings.repeat = !repeat;
                self.progress.save();
            }
            if icon_button(ptr, r_restart, PANEL, |c| icon_to_start(c, 24.0, TEXT)) {
                p.practice.restart(&self.audio);
                p.result = None;
                p.last_run = None;
            }
            let paused = p.practice.paused;
            if icon_button(ptr, r_pause, PANEL, |c| {
                if paused {
                    icon_play(c, 22.0, TEXT)
                } else {
                    icon_pause(c, 22.0, TEXT)
                }
            }) {
                p.practice.set_paused(!paused, &self.audio);
            }
        }

        // Progress bar
        draw_rectangle(0.0, wf.y, w, 4.0, PANEL);
        draw_rectangle(0.0, wf.y, w * p.practice.fraction(), 4.0, diff_col);

        if let Some(sum) = p.result {
            let is_last = p.lesson + 1 >= level.lessons.len();
            match self.result_overlay(&sum, p.practice.mode, is_last, p.fireworks.as_ref()) {
                Some(true) => {
                    p.practice.restart(&self.audio);
                    p.result = None;
                }
                Some(false) if is_last => action = Some(Action::OpenComposition(p.comp)),
                Some(false) => action = Some(Action::OpenLesson(p.comp, p.level, p.lesson + 1)),
                None => {}
            }
        }
        action
    }

    fn draw_waterfall(&self, pr: &Practice, keys: &KeyLayout, wf: Rect) {
        draw_rectangle(wf.x, wf.y, wf.w, wf.h, BG_DEEP);
        // Lane guides at every C and F
        for p in keys.pitches() {
            if p % 12 == 0 || p % 12 == 5 {
                if let Some(k) = keys.key(p) {
                    let c = if p % 12 == 0 { alpha(WHITE, 0.09) } else { alpha(WHITE, 0.04) };
                    draw_line(k.x, wf.y, k.x, wf.y + wf.h, 1.0, c);
                }
            }
        }
        let pps = wf.h as f64 / LOOKAHEAD;
        let y_hit = (wf.y + wf.h) as f64;
        let t = pr.t;
        let mut bar_numbers: Vec<(&str, f32)> = vec![];
        for (k, &m) in pr.score.measures.iter().enumerate() {
            let y = (y_hit - (m - t) * pps) as f32;
            if y >= wf.y && y <= wf.y + wf.h {
                draw_line(wf.x, y, wf.x + wf.w, y, 1.0, alpha(WHITE, 0.10));
                if let Some(num) = pr.score.measure_numbers.get(k).filter(|n| !n.is_empty() && *n != "0") {
                    bar_numbers.push((num, y));
                }
            }
        }

        let pulse = ((get_time() * 5.0).sin() * 0.5 + 0.5) as f32;
        let notes = &pr.score.notes;
        // Text is drawn after all shapes: mixing them splits the frame into many
        // small GPU draw calls, which makes old Intel GPUs flicker.
        let mut labels: Vec<(u8, f32, f32, f32)> = vec![];
        // White-key notes first so black-key notes stay visible on top.
        for black_pass in [false, true] {
            for (i, n) in notes.iter().enumerate() {
                if n.start > t + LOOKAHEAD + 0.1 {
                    break;
                }
                if n.end < t || is_black(n.pitch) != black_pass {
                    continue;
                }
                let Some((x, lw)) = keys.lane(n.pitch) else { continue };
                let bottom = (y_hit - (n.start - t) * pps) as f32;
                let top = (y_hit - (n.end - t) * pps) as f32;
                let (top_c, bot_c) = (top.max(wf.y), bottom.min(wf.y + wf.h));
                if bot_c - top_c < 1.0 {
                    continue;
                }
                let base = hand_color(pr.hand(i));
                let color = if !pr.is_user[i] {
                    alpha(mix(base, BG_DEEP, 0.35), 0.55)
                } else {
                    match pr.judge[i] {
                        Judge::Hit(_) => mix(base, GOOD, 0.55),
                        Judge::Missed => mix(base, BAD, 0.6),
                        Judge::Pending => base,
                    }
                };
                let r = Rect::new(x, top_c, lw, bot_c - top_c);
                if pr.group.contains(&i) {
                    let g = 3.0 + 3.0 * pulse;
                    rounded_rect(Rect::new(r.x - g, r.y - g, r.w + 2.0 * g, r.h + 2.0 * g), 9.0, alpha(WHITE, 0.25 + 0.35 * pulse));
                }
                rounded_rect(r, 7.0, color);
                let rad = (lw * 0.36).min(13.0);
                if let Some(fg) = n.finger {
                    let tall = r.h >= rad * 2.0 + 4.0 || pr.group.contains(&i);
                    if tall && (pr.is_user[i] || pr.mode == Mode::Listen) {
                        let cy = (bot_c - rad - 4.0).max(r.y + rad);
                        draw_circle(x + lw / 2.0, cy, rad, alpha(WHITE, 0.92));
                        labels.push((fg, x + lw / 2.0, cy, rad * 1.45));
                    }
                }
            }
        }
        // Hit line
        draw_rectangle(wf.x, wf.y + wf.h - 3.0, wf.w, 3.0, alpha(WHITE, 0.25));
        for (fg, x, y, size) in labels {
            text_center(&self.fonts.bold, &fg.to_string(), x, y, size, BG);
        }
        for (num, y) in bar_numbers {
            text_left(&self.fonts.regular, num, wf.x + 8.0, y - 10.0, 14.0, alpha(WHITE, 0.4));
        }
    }

    fn draw_keyboard(&self, pr: &Practice, keys: &KeyLayout) {
        let kb = keys.area;
        draw_rectangle(kb.x, kb.y, kb.w, kb.h, rgb(0x0b0c11));
        let pulse = ((get_time() * 5.0).sin() * 0.5 + 0.5) as f32;

        let mut auto = [None::<Hand>; 128];
        for (i, n) in pr.score.notes.iter().enumerate() {
            if !pr.is_user[i] && n.start <= pr.t && n.end > pr.t {
                auto[n.pitch as usize] = Some(pr.hand(i));
            }
        }
        let mut target = [None::<(Hand, Option<u8>)>; 128];
        if pr.mode == Mode::Wait {
            for &i in &pr.group {
                let n = &pr.score.notes[i];
                target[n.pitch as usize] = Some((pr.hand(i), n.finger));
            }
        } else if pr.mode == Mode::Realtime {
            for (i, n) in pr.score.notes.iter().enumerate() {
                if pr.is_user[i] && pr.judge[i] == Judge::Pending && (n.start - pr.t).abs() < 0.2 * pr.tempo {
                    target[n.pitch as usize] = Some((pr.hand(i), n.finger));
                }
            }
        }

        let key_color = |p: u8, base: Color| -> Color {
            let i = p as usize;
            if pr.wrong_flash[i] > 0.0 {
                BAD
            } else if pr.held[i] {
                if pr.hit_flash[i] > 0.0 || target[i].is_some() { GOOD } else { mix(base, RIGHT_HAND, 0.35) }
            } else if let Some(h) = auto[i] {
                mix(base, hand_color(h), 0.65)
            } else if let Some((h, _)) = target[i] {
                mix(base, hand_color(h), 0.3 + 0.45 * pulse)
            } else {
                base
            }
        };

        // Shapes first, text afterwards (see draw_waterfall).
        let mut names: Vec<(String, f32, f32, f32)> = vec![];
        let mut fingers: Vec<(u8, f32, f32, f32)> = vec![];
        for black in [false, true] {
            for p in keys.pitches().filter(|&p| is_black(p) == black) {
                let k = keys.key(p).unwrap();
                let (base, r) = if black {
                    (rgb(0x1c1d22), Rect::new(k.x, k.y - 4.0, k.w, k.h + 4.0))
                } else {
                    (rgb(0xf4f4f0), Rect::new(k.x + 1.0, k.y, k.w - 2.0, k.h - 2.0))
                };
                rounded_rect(r, if black { 4.0 } else { 6.0 }, key_color(p, base));
                if !black && p % 12 == 0 {
                    let name = keyboard::note_name(p);
                    let fs = fit_size(&self.fonts.regular, &name, 15.0, k.w - 6.0);
                    names.push((name, k.x + k.w / 2.0, k.y + k.h - 16.0, fs));
                }
                if let Some((h, Some(fg))) = target[p as usize] {
                    let rad = (k.w * 0.34).min(15.0);
                    let cy = if black { k.y + k.h - rad - 8.0 } else { k.y + k.h - rad - 32.0 };
                    draw_circle(k.x + k.w / 2.0, cy, rad, hand_color(h));
                    fingers.push((fg, k.x + k.w / 2.0, cy, rad * 1.4));
                }
            }
        }
        // Felt strip
        draw_rectangle(kb.x, kb.y - 4.0, kb.w, 4.0, rgb(0x7a1f2b));
        for (name, x, y, size) in names {
            text_center(&self.fonts.regular, &name, x, y, size, MUTED);
        }
        for (fg, x, y, size) in fingers {
            text_center(&self.fonts.bold, &fg.to_string(), x, y, size, BG);
        }
    }

    /// Returns Some(true) = repeat, Some(false) = next.
    fn result_overlay(&self, sum: &Summary, mode: Mode, is_last: bool, fireworks: Option<&Fireworks>) -> Option<bool> {
        let s = self.s();
        let f = &self.fonts;
        let ptr = &self.pointer;
        let (w, h) = (vw(), vh());
        draw_rectangle(0.0, 0.0, w, h, alpha(BLACK, 0.6));
        if let Some(fx) = fireworks {
            fx.draw();
        }
        let pw = 560.0f32.min(w - 32.0);
        let ph = if mode == Mode::Listen { 230.0 } else { 420.0f32.min(h - 32.0) };
        let panel = Rect::new((w - pw) / 2.0, (h - ph) / 2.0, pw, ph);
        rounded_rect(panel, 22.0, PANEL);
        let cx = w / 2.0;
        let mut y = panel.y + 52.0;
        if mode == Mode::Listen {
            text_center(&f.bold, s.listen_done, cx, y, 32.0, TEXT);
        } else {
            text_center(&f.bold, s.grades[sum.stars as usize], cx, y, 34.0, TEXT);
            y += 72.0;
            stars_row(cx, y, 30.0, sum.stars);
            y += 78.0;
            text_center(&f.bold, &format!("{:.0} %", sum.percent), cx, y, 46.0, TEXT);
            y += 50.0;
            let mut line = format!(
                "{} {}/{}  ·  {} {}",
                s.hits, sum.hits, sum.total, s.wrong, sum.wrong
            );
            if mode == Mode::Realtime {
                line += &format!("  ·  {} {}", s.missed, sum.missed);
            }
            if let Some(ms) = sum.avg_timing_ms {
                line += &format!("  ·  {} ±{:.0} ms", s.timing, ms);
            }
            let fs = fit_size(&f.regular, &line, 19.0, pw - 40.0);
            text_center(&f.regular, &line, cx, y, fs, MUTED);
        }
        let bw = (pw - 72.0) / 2.0;
        let by = panel.y + panel.h - 88.0;
        let r1 = Rect::new(panel.x + 24.0, by, bw, 62.0);
        let r2 = Rect::new(panel.x + 48.0 + bw, by, bw, 62.0);
        if button(f, ptr, r1, s.repeat, PANEL_HI, TEXT, 22.0) {
            return Some(true);
        }
        if button(f, ptr, r2, if is_last { s.done } else { s.next }, GOOD, BG, 22.0) {
            return Some(false);
        }
        None
    }

    // -------------------------------------------------------------- settings

    fn settings(&mut self, ptr: Pointer) {
        let s = self.s();
        let (w, h) = (vw(), vh());
        draw_rectangle(0.0, 0.0, w, h, alpha(BLACK, 0.6));
        let pw = 620.0f32.min(w - 32.0);
        let ph = 530.0f32.min(h - 32.0);
        let panel = Rect::new((w - pw) / 2.0, (h - ph) / 2.0, pw, ph);
        rounded_rect(panel, 22.0, PANEL);
        let f = &self.fonts;
        text_left(&f.bold, s.settings, panel.x + 32.0, panel.y + 48.0, 30.0, TEXT);

        let label_w = pw * 0.45;
        let seg_w = pw - label_w - 56.0;
        let row = |i: usize| panel.y + 110.0 + i as f32 * 76.0;
        let seg_rect = |i: usize| Rect::new(panel.x + 24.0 + label_w, row(i) - 26.0, seg_w, 52.0);

        let mut changed = false;
        text_left(&f.regular, s.language, panel.x + 32.0, row(0), 20.0, TEXT);
        let li = if self.lang == Lang::De { 0 } else { 1 };
        if let Some(i) = segmented(f, &ptr, seg_rect(0), &["Deutsch", "English"], li, TEXT, 18.0) {
            self.lang = if i == 0 { Lang::De } else { Lang::En };
            self.progress.settings.language = Some(self.lang);
            changed = true;
        }
        let f = &self.fonts;
        let lw = fit_size(&f.regular, s.echo, 20.0, label_w - 12.0);
        text_left(&f.regular, s.echo, panel.x + 32.0, row(1), lw, TEXT);
        let echo = self.progress.settings.echo_input;
        if let Some(i) = segmented(f, &ptr, seg_rect(1), &[s.on, s.off], if echo { 0 } else { 1 }, TEXT, 18.0) {
            self.progress.settings.echo_input = i == 0;
            self.echo.store(i == 0, Ordering::Relaxed);
            changed = true;
        }
        text_left(&f.regular, s.fullscreen, panel.x + 32.0, row(2), 20.0, TEXT);
        let fsn = self.progress.settings.fullscreen;
        if let Some(i) = segmented(f, &ptr, seg_rect(2), &[s.on, s.off], if fsn { 0 } else { 1 }, TEXT, 18.0) {
            self.progress.settings.fullscreen = i == 0;
            set_fullscreen(i == 0);
            changed = true;
        }
        // Seven choices don't fit next to the label: give them the full width.
        text_left(&f.regular, s.keyboard_size, panel.x + 32.0, row(3), 20.0, TEXT);
        let sizes = KeyboardSize::ALL;
        let labels = sizes.map(|k| if k == KeyboardSize::Auto { s.auto } else { k.label() });
        let ki = sizes.iter().position(|&k| k == self.progress.settings.keyboard).unwrap_or(0);
        let kb_rect = Rect::new(panel.x + 24.0, row(3) + 26.0, pw - 48.0, 52.0);
        if let Some(i) = segmented(f, &ptr, kb_rect, &labels, ki, TEXT, 18.0) {
            self.progress.settings.keyboard = sizes[i];
            changed = true;
        }
        let close = Rect::new(panel.x + pw - 24.0 - 180.0, panel.y + ph - 84.0, 180.0, 60.0);
        if button(f, &ptr, close, s.close, PANEL_HI, TEXT, 20.0) || (ptr.released && !panel.contains(ptr.pos) && !panel.contains(ptr.press_pos)) {
            self.show_settings = false;
        }
        if changed {
            self.progress.save();
        }
    }

    fn error(&self, e: &str, ptr: &Pointer) -> Option<Action> {
        let (w, h) = (vw(), vh());
        let f = &self.fonts;
        let fs = fit_size(&f.regular, e, 20.0, w - 40.0);
        text_center(&f.regular, e, w / 2.0, h / 2.0 - 40.0, fs, BAD);
        if self.lib.is_some() {
            let r = Rect::new(w / 2.0 - 100.0, h / 2.0 + 20.0, 200.0, 60.0);
            if button(f, ptr, r, self.s().back, PANEL_HI, TEXT, 20.0) {
                return Some(Action::Home);
            }
        }
        None
    }
}
