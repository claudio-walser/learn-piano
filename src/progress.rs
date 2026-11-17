//! Settings and best results, stored as JSON in the user's data directory.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::keyboard::KeyboardSize;
use crate::practice::{Hands, Mode, Players};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Language code like "de"; `None` = follow the system language.
    pub language: Option<String>,
    pub echo_input: bool,
    pub fullscreen: bool,
    pub mode: Mode,
    pub hands: Hands,
    /// Who plays a duet.
    pub players: Players,
    pub tempo: f64,
    /// On-screen keyboard size; `Auto` fits it to the lesson.
    pub keyboard: KeyboardSize,
    /// Start the lesson again automatically when it ends.
    pub repeat: bool,
    /// Library folder chosen in the settings; `None` = search automatically.
    pub library: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: None,
            echo_input: true,
            fullscreen: false,
            mode: Mode::Wait,
            hands: Hands::Both,
            players: Players::Student,
            tempo: 1.0,
            keyboard: KeyboardSize::Auto,
            repeat: false,
            library: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct LessonResult {
    pub best_stars: u8,
    pub best_percent: f32,
    pub plays: u32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Progress {
    pub settings: Settings,
    /// Key: `<composition>/<difficulty>/<lesson file stem>`.
    pub results: BTreeMap<String, LessonResult>,
}

/// `~/.local/share/openetude` (or the platform's equivalent). The folder of the app's former
/// name `learn-piano` is moved there once, so progress and settings are kept.
pub fn data_dir() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    let dir = base.join("openetude");
    let old = base.join("learn-piano");
    if !dir.exists() && old.is_dir() {
        if let Err(e) = std::fs::rename(&old, &dir) {
            eprintln!("{} -> {}: {e}", old.display(), dir.display());
            return Some(old);
        }
    }
    Some(dir)
}

fn path() -> Option<PathBuf> {
    Some(data_dir()?.join("progress.json"))
}

impl Progress {
    pub fn load() -> Progress {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(p.parent().unwrap())?;
            let tmp = p.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
            std::fs::rename(tmp, &p)
        };
        if let Err(e) = write() {
            eprintln!("saving progress failed: {e}");
        }
    }

    pub fn get(&self, key: &str) -> Option<LessonResult> {
        self.results.get(key).copied()
    }

    pub fn stars(&self, key: &str) -> u8 {
        self.get(key).map_or(0, |r| r.best_stars)
    }

    pub fn record(&mut self, key: String, stars: u8, percent: f32) {
        let r = self.results.entry(key).or_default();
        r.plays += 1;
        r.best_stars = r.best_stars.max(stars);
        r.best_percent = r.best_percent.max(percent);
        self.save();
    }
}
