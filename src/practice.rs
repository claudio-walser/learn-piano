//! Practice session logic: timing, waiting for the player, judging, autoplay.

use serde::{Deserialize, Serialize};

use crate::audio::Audio;
use crate::score::{Note, Score};

/// Score seconds shown before the first note reaches the keyboard.
pub const LEAD_IN: f64 = 2.5;
/// Notes starting within this window form one chord.
const CHORD_EPS: f64 = 0.04;
/// Real-time mode: max. deviation (real seconds) that still counts as a hit.
const RT_TOLERANCE: f64 = 0.25;
/// Real-time mode: deviation that still counts as perfect.
const RT_PERFECT: f64 = 0.08;
/// MIDI channel used for notes the app plays itself.
pub const AUTO_CHANNEL: u8 = 1;
const AUTO_VELOCITY: u8 = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// The music stops until the right keys are pressed.
    Wait,
    /// The music runs at tempo; hits and timing are judged.
    Realtime,
    /// The app plays everything.
    Listen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hands {
    Left,
    Right,
    Both,
}

/// Who plays a duet. Ignored for scores with a single part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Players {
    /// The player takes part 0, the app plays the teacher part.
    Student,
    /// The player takes part 1, the app plays the student part.
    Teacher,
    /// Two people at one keyboard: every part is played and judged.
    Together,
}

impl Players {
    fn plays(self, n: &Note, duet: bool) -> bool {
        match self {
            _ if !duet => true,
            Players::Student => n.part == 0,
            Players::Teacher => n.part == 1,
            Players::Together => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Judge {
    Pending,
    /// Timing error in real seconds (negative = early).
    Hit(f64),
    Missed,
}

#[derive(Clone, Copy, Debug)]
pub struct Summary {
    pub total: u32,
    pub hits: u32,
    pub wrong: u32,
    pub missed: u32,
    pub avg_timing_ms: Option<f32>,
    pub percent: f32,
    pub stars: u8,
}

pub fn hand_of(n: &Note, staves: u8) -> Hand {
    if staves >= 2 && n.staff >= 2 {
        Hand::Left
    } else {
        Hand::Right
    }
}

pub struct Practice {
    pub score: Score,
    pub mode: Mode,
    pub hands: Hands,
    pub players: Players,
    pub tempo: f64,
    /// Current position in score seconds.
    pub t: f64,
    pub judge: Vec<Judge>,
    /// Whether the player plays this note (otherwise the app does).
    pub is_user: Vec<bool>,
    user_idx: Vec<usize>,
    next_user: usize,
    auto_idx: Vec<usize>,
    next_auto: usize,
    sounding: Vec<(f64, u8)>,
    /// Wait mode: the chord the app is waiting for.
    pub group: Vec<usize>,
    pub waiting: bool,
    pub wrong: u32,
    pub paused: bool,
    pub finished: bool,
    /// Jumped around with [`Practice::seek`]: the run does not count as an attempt.
    pub seeked: bool,
    pub held: [bool; 128],
    pub wrong_flash: [f32; 128],
    pub hit_flash: [f32; 128],
}

impl Practice {
    pub fn new(score: Score, mode: Mode, hands: Hands, players: Players, tempo: f64) -> Practice {
        let mut p = Practice {
            score,
            mode,
            hands,
            players,
            tempo,
            t: -LEAD_IN,
            judge: Vec::new(),
            is_user: Vec::new(),
            user_idx: Vec::new(),
            next_user: 0,
            auto_idx: Vec::new(),
            next_auto: 0,
            sounding: Vec::new(),
            group: Vec::new(),
            waiting: false,
            wrong: 0,
            paused: false,
            finished: false,
            seeked: false,
            held: [false; 128],
            wrong_flash: [0.0; 128],
            hit_flash: [0.0; 128],
        };
        p.reset();
        p
    }

    pub fn hand(&self, i: usize) -> Hand {
        hand_of(&self.score.notes[i], self.score.staves)
    }

    fn reset(&mut self) {
        let (staves, duet) = (self.score.staves, self.score.is_duet());
        let (mode, hands, players) = (self.mode, self.hands, self.players);
        self.is_user = self
            .score
            .notes
            .iter()
            .map(|n| {
                mode != Mode::Listen
                    && players.plays(n, duet)
                    && match hands {
                        Hands::Both => true,
                        Hands::Right => hand_of(n, staves) == Hand::Right,
                        Hands::Left => hand_of(n, staves) == Hand::Left,
                    }
            })
            .collect();
        let n = self.score.notes.len();
        self.user_idx = (0..n).filter(|&i| self.is_user[i]).collect();
        self.auto_idx = (0..n).filter(|&i| !self.is_user[i]).collect();
        self.judge = vec![Judge::Pending; n];
        self.t = -LEAD_IN;
        self.next_user = 0;
        self.next_auto = 0;
        self.sounding.clear();
        self.group.clear();
        self.waiting = false;
        self.wrong = 0;
        self.paused = false;
        self.finished = false;
        self.seeked = false;
        self.wrong_flash = [0.0; 128];
        self.hit_flash = [0.0; 128];
        self.recompute_group();
    }

    pub fn restart(&mut self, audio: &Audio) {
        audio.all_off();
        self.reset();
    }

    pub fn configure(&mut self, mode: Mode, hands: Hands, players: Players, audio: &Audio) {
        self.mode = mode;
        self.hands = hands;
        self.players = players;
        self.restart(audio);
    }

    pub fn set_paused(&mut self, paused: bool, audio: &Audio) {
        self.paused = paused;
        if paused {
            for (_, p) in self.sounding.drain(..) {
                audio.note_off(AUTO_CHANNEL, p);
            }
        }
    }

    /// Jumps to score time `t`. Notes from there on are open again; autoplay
    /// continues from there.
    pub fn seek(&mut self, t: f64, audio: &Audio) {
        for (_, p) in self.sounding.drain(..) {
            audio.note_off(AUTO_CHANNEL, p);
        }
        let t = t.clamp(-LEAD_IN, self.score.duration);
        let notes = &self.score.notes;
        let from = t - CHORD_EPS;
        for (i, n) in notes.iter().enumerate() {
            if n.start >= from {
                self.judge[i] = Judge::Pending;
            }
        }
        self.next_user = self.user_idx.partition_point(|&i| notes[i].start < from);
        self.next_auto = self.auto_idx.partition_point(|&i| notes[i].start < t);
        self.t = t;
        self.seeked = true;
        self.finished = false;
        self.waiting = false;
        self.recompute_group();
    }

    pub fn fraction(&self) -> f32 {
        (self.t / self.score.duration.max(0.001)).clamp(0.0, 1.0) as f32
    }

    fn recompute_group(&mut self) {
        while self
            .user_idx
            .get(self.next_user)
            .is_some_and(|&i| self.judge[i] != Judge::Pending)
        {
            self.next_user += 1;
        }
        self.group.clear();
        if self.mode != Mode::Wait {
            return;
        }
        if let Some(&first) = self.user_idx.get(self.next_user) {
            let s0 = self.score.notes[first].start;
            for &i in &self.user_idx[self.next_user..] {
                if self.score.notes[i].start > s0 + CHORD_EPS {
                    break;
                }
                if self.judge[i] == Judge::Pending {
                    self.group.push(i);
                }
            }
        }
    }

    pub fn key_down(&mut self, pitch: u8) {
        let p = pitch as usize & 127;
        self.held[p] = true;
        if self.paused || self.finished || self.mode == Mode::Listen || self.t < -LEAD_IN + 0.5 {
            return;
        }
        let notes = &self.score.notes;
        let hit = match self.mode {
            Mode::Wait => self.group.iter().copied().find(|&i| notes[i].pitch == pitch).map(|i| (i, 0.0)),
            Mode::Realtime => {
                let tol = RT_TOLERANCE * self.tempo;
                self.user_idx[self.next_user..]
                    .iter()
                    .copied()
                    .take_while(|&i| notes[i].start <= self.t + tol)
                    .filter(|&i| {
                        self.judge[i] == Judge::Pending
                            && notes[i].pitch == pitch
                            && (notes[i].start - self.t).abs() <= tol
                    })
                    .min_by(|&a, &b| {
                        (notes[a].start - self.t).abs().total_cmp(&(notes[b].start - self.t).abs())
                    })
                    .map(|i| (i, (self.t - notes[i].start) / self.tempo))
            }
            Mode::Listen => None,
        };
        match hit {
            Some((i, err)) => {
                self.judge[i] = Judge::Hit(err);
                self.hit_flash[p] = 0.35;
                self.recompute_group();
            }
            None => {
                self.wrong += 1;
                self.wrong_flash[p] = 0.45;
            }
        }
    }

    pub fn key_up(&mut self, pitch: u8) {
        self.held[pitch as usize & 127] = false;
    }

    pub fn update(&mut self, dt: f32, audio: &Audio) {
        for f in self.wrong_flash.iter_mut().chain(self.hit_flash.iter_mut()) {
            *f = (*f - dt).max(0.0);
        }
        if self.paused || self.finished {
            return;
        }
        let adv = dt as f64 * self.tempo;
        match self.mode {
            Mode::Wait => {
                self.recompute_group();
                match self.group.first() {
                    Some(&g) if self.t + adv >= self.score.notes[g].start => {
                        self.t = self.t.max(self.score.notes[g].start);
                        self.waiting = true;
                    }
                    _ => {
                        self.t += adv;
                        self.waiting = false;
                    }
                }
            }
            Mode::Realtime => {
                self.t += adv;
                let limit = self.t - RT_TOLERANCE * self.tempo;
                for &i in &self.user_idx[self.next_user..] {
                    if self.score.notes[i].start >= limit {
                        break;
                    }
                    if self.judge[i] == Judge::Pending {
                        self.judge[i] = Judge::Missed;
                    }
                }
                self.recompute_group();
            }
            Mode::Listen => self.t += adv,
        }

        // Autoplay: release first, so a repeated pitch is not cut off.
        let t = self.t;
        self.sounding.retain(|&(end, p)| {
            let keep = end > t;
            if !keep {
                audio.note_off(AUTO_CHANNEL, p);
            }
            keep
        });
        while let Some(&i) = self.auto_idx.get(self.next_auto) {
            let n = &self.score.notes[i];
            if n.start > t {
                break;
            }
            if n.end > t {
                audio.note_on(AUTO_CHANNEL, n.pitch, AUTO_VELOCITY);
                self.sounding.push((n.end, n.pitch));
            }
            self.next_auto += 1;
        }

        if t >= self.score.duration + 0.5 && self.next_user >= self.user_idx.len() {
            self.finished = true;
            for (_, p) in self.sounding.drain(..) {
                audio.note_off(AUTO_CHANNEL, p);
            }
        }
    }

    pub fn summary(&self) -> Summary {
        let total = self.user_idx.len() as u32;
        let mut hits = 0;
        let mut missed = 0;
        let mut points = 0.0;
        let mut timing_sum = 0.0;
        for &i in &self.user_idx {
            match self.judge[i] {
                Judge::Hit(err) => {
                    hits += 1;
                    timing_sum += err.abs();
                    points += if self.mode == Mode::Realtime {
                        let over = ((err.abs() - RT_PERFECT) / (RT_TOLERANCE - RT_PERFECT)).clamp(0.0, 1.0);
                        1.0 - 0.5 * over
                    } else {
                        1.0
                    };
                }
                Judge::Missed => missed += 1,
                Judge::Pending => {}
            }
        }
        let denom = (total + self.wrong) as f64;
        let percent = if denom == 0.0 { 100.0 } else { (100.0 * points / denom) as f32 };
        let stars = match percent {
            p if p >= 90.0 => 3,
            p if p >= 75.0 => 2,
            p if p >= 50.0 => 1,
            _ => 0,
        };
        let avg_timing_ms = (self.mode == Mode::Realtime && hits > 0)
            .then(|| (timing_sum / hits as f64 * 1000.0) as f32);
        Summary { total, hits, wrong: self.wrong, missed, avg_timing_ms, percent, stars }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score() -> Score {
        let n = |pitch, start: f64, staff| Note { pitch, start, end: start + 0.5, staff, finger: None, part: 0 };
        Score {
            title: None,
            notes: vec![n(60, 0.0, 1), n(48, 0.0, 2), n(62, 1.0, 1)],
            measures: vec![0.0],
            measure_numbers: vec!["1".into()],
            duration: 1.5,
            staves: 2,
            parts: vec!["Piano".into()],
        }
    }

    #[test]
    fn wait_mode_waits_for_right_keys() {
        let audio = Audio::start(None);
        let mut p = Practice::new(score(), Mode::Wait, Hands::Right, Players::Student, 1.0);
        for _ in 0..400 {
            p.update(0.01, &audio);
        }
        assert!(p.waiting);
        assert_eq!(p.t, 0.0);
        p.key_down(61); // wrong
        p.key_down(60);
        for _ in 0..200 {
            p.update(0.01, &audio);
        }
        assert!((p.t - 1.0).abs() < 1e-9);
        p.key_down(62);
        for _ in 0..200 {
            p.update(0.01, &audio);
        }
        assert!(p.finished);
        let s = p.summary();
        assert_eq!((s.total, s.hits, s.wrong), (2, 2, 1));
        assert_eq!(s.stars, 1); // 2 / 3 = 66 %
    }

    #[test]
    fn duet_players_choose_the_part() {
        let mut sc = score();
        sc.notes[1].part = 1; // the left-hand C3 belongs to the teacher
        sc.parts.push("Teacher".into());
        let user = |players, hands| Practice::new(sc.clone(), Mode::Wait, hands, players, 1.0).is_user;
        assert_eq!(user(Players::Student, Hands::Both), [true, false, true]);
        assert_eq!(user(Players::Teacher, Hands::Both), [false, true, false]);
        assert_eq!(user(Players::Together, Hands::Both), [true, true, true]);
        assert_eq!(user(Players::Together, Hands::Right), [true, false, true]);
        // Single-part scores ignore the choice.
        let solo = Practice::new(score(), Mode::Wait, Hands::Both, Players::Teacher, 1.0);
        assert_eq!(solo.is_user, [true, true, true]);
    }

    #[test]
    fn seek_reopens_notes_and_moves_autoplay() {
        let audio = Audio::start(None);
        let mut p = Practice::new(score(), Mode::Wait, Hands::Right, Players::Student, 1.0);
        p.seek(0.5, &audio);
        assert!(p.seeked);
        // Waits for the D at 1.0 next, the C at 0.0 is behind us.
        for _ in 0..100 {
            p.update(0.01, &audio);
        }
        assert!(p.waiting);
        assert_eq!(p.t, 1.0);
        assert_eq!(p.group, [2]);
        // Back to the start: the C is due again.
        p.key_down(62);
        p.seek(-1.0, &audio);
        assert_eq!(p.judge[2], Judge::Pending);
        for _ in 0..200 {
            p.update(0.01, &audio);
        }
        assert_eq!(p.group, [0]);
        p.restart(&audio);
        assert!(!p.seeked);
    }

    #[test]
    fn realtime_judges_timing_and_misses() {
        let audio = Audio::start(None);
        let mut p = Practice::new(score(), Mode::Realtime, Hands::Both, Players::Student, 1.0);
        while p.t < 0.05 {
            p.update(0.01, &audio);
        }
        p.key_down(60);
        p.key_down(48);
        for _ in 0..500 {
            p.update(0.01, &audio);
        }
        assert!(p.finished);
        let s = p.summary();
        assert_eq!((s.hits, s.missed, s.wrong), (2, 1, 0));
    }
}
