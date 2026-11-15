//! Settings and best results, stored as JSON in the user's data directory.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::practice::{Hands, Mode};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// `None` = follow the system language.
    pub language: Option<Lang>,
    pub echo_input: bool,
    pub fullscreen: bool,
    pub mode: Mode,
    pub hands: Hands,
    pub tempo: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: None,
            echo_input: true,
            fullscreen: false,
            mode: Mode::Wait,
            hands: Hands::Both,
            tempo: 1.0,
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

fn path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("learn-piano").join("progress.json"))
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
