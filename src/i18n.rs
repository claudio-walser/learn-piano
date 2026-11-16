//! App texts with Fluent: `locales/<lang>/learn_piano.ftl`, embedded into the binary.
//!
//! English is the source language and the fallback for missing messages. Every message id
//! used in [`Strings::new`] is checked against the English file at compile time by `fl!`.
//! The available languages are the folders in `locales/`; nothing here lists them.

use std::collections::HashMap;

use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::LanguageLoader;
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use unic_langid::LanguageIdentifier;

#[derive(RustEmbed)]
#[folder = "locales/"]
struct Localizations;

/// All texts of the app in one language.
///
/// The texts are `&'static str` so that drawing code can use them without allocating or
/// borrowing the app. Each language is built once and kept for the rest of the run.
pub struct Strings {
    loader: FluentLanguageLoader,
    pub app_title: &'static str,
    pub choose_difficulty: &'static str,
    pub mode_wait: &'static str,
    pub mode_realtime: &'static str,
    pub mode_listen: &'static str,
    pub hand_left: &'static str,
    pub hand_right: &'static str,
    pub hand_both: &'static str,
    pub players: [&'static str; 3],
    pub full_piece: &'static str,
    pub repeat: &'static str,
    pub next: &'static str,
    pub done: &'static str,
    pub hits: &'static str,
    pub wrong: &'static str,
    pub missed: &'static str,
    pub timing: &'static str,
    pub grades: [&'static str; 4],
    pub listen_done: &'static str,
    pub loading_sound: &'static str,
    pub sound_failed: &'static str,
    pub no_midi: &'static str,
    pub paused: &'static str,
    pub settings: &'static str,
    pub language: &'static str,
    pub echo: &'static str,
    pub fullscreen: &'static str,
    pub keyboard_size: &'static str,
    pub auto: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub close: &'static str,
    pub no_compositions: &'static str,
    pub back: &'static str,
    pub library: &'static str,
    pub library_auto: &'static str,
    pub change: &'static str,
    pub choose_folder: &'static str,
    pub use_folder: &'static str,
    pub cancel: &'static str,
    pub default_folder: &'static str,
    pub library_here: &'static str,
    pub no_library_here: &'static str,
    pub no_library: &'static str,
    pub all: &'static str,
    pub search: &'static str,
    pub no_matches: &'static str,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Strings {
    fn new(l: FluentLanguageLoader) -> Strings {
        macro_rules! t {
            ($id:literal) => {
                leak(fl!(l, $id))
            };
        }
        Strings {
            app_title: t!("app-title"),
            choose_difficulty: t!("choose-difficulty"),
            mode_wait: t!("mode-wait"),
            mode_realtime: t!("mode-realtime"),
            mode_listen: t!("mode-listen"),
            hand_left: t!("hand-left"),
            hand_right: t!("hand-right"),
            hand_both: t!("hand-both"),
            players: [t!("player-student"), t!("player-teacher"), t!("player-together")],
            full_piece: t!("full-piece"),
            repeat: t!("repeat"),
            next: t!("next"),
            done: t!("done"),
            hits: t!("hits"),
            wrong: t!("wrong"),
            missed: t!("missed"),
            timing: t!("timing"),
            grades: [t!("grade-0"), t!("grade-1"), t!("grade-2"), t!("grade-3")],
            listen_done: t!("listen-done"),
            loading_sound: t!("loading-sound"),
            sound_failed: t!("sound-failed"),
            no_midi: t!("no-midi"),
            paused: t!("paused"),
            settings: t!("settings"),
            language: t!("language"),
            echo: t!("echo"),
            fullscreen: t!("fullscreen"),
            keyboard_size: t!("keyboard-size"),
            auto: t!("auto"),
            on: t!("on"),
            off: t!("off"),
            close: t!("close"),
            no_compositions: t!("no-compositions"),
            back: t!("back"),
            library: t!("library"),
            library_auto: t!("library-auto"),
            change: t!("change"),
            choose_folder: t!("choose-folder"),
            use_folder: t!("use-folder"),
            cancel: t!("cancel"),
            default_folder: t!("default-folder"),
            library_here: t!("library-here"),
            no_library_here: t!("no-library-here"),
            no_library: t!("no-library"),
            all: t!("all"),
            search: t!("search"),
            no_matches: t!("no-matches"),
            loader: l,
        }
    }

    /// "4 lessons" with the plural rules of the language.
    pub fn lesson_count(&self, count: usize) -> String {
        fl!(self.loader, "lesson-count", count = count)
    }

    /// "Part A".
    pub fn part(&self, label: &str) -> String {
        fl!(self.loader, "part", label = label)
    }
}

/// The app's languages and the texts built so far.
pub struct I18n {
    loader: FluentLanguageLoader,
    /// Available languages with their own names ("Deutsch"), sorted by name.
    pub languages: Vec<(LanguageIdentifier, String)>,
    cache: HashMap<LanguageIdentifier, &'static Strings>,
}

impl I18n {
    pub fn load() -> I18n {
        let loader: FluentLanguageLoader = fluent_language_loader!();
        let mut available = loader.available_languages(&Localizations).unwrap_or_default();
        // A broken translation must not take the app down: fall back to English only.
        if loader.load_languages(&Localizations, &available).is_err() {
            available = vec![loader.fallback_language().clone()];
            loader.load_languages(&Localizations, &available).expect("English texts are embedded");
        }
        // Fluent wraps arguments in Unicode isolation marks, which the font draws as boxes.
        loader.set_use_isolating(false);
        let mut languages: Vec<(LanguageIdentifier, String)> = available
            .into_iter()
            .map(|id| {
                let name = loader.select_languages(&[&id]).get("language-name");
                (id, name)
            })
            .collect();
        languages.sort_by_key(|(_, name)| name.to_lowercase());
        I18n { loader, languages, cache: HashMap::new() }
    }

    /// The best available language for `wanted` (from the settings) or else the system
    /// languages: exact match first, then the same language without region, else English.
    pub fn resolve(&self, wanted: Option<&str>) -> LanguageIdentifier {
        let requested: Vec<LanguageIdentifier> = match wanted {
            Some(w) => w.parse().ok().into_iter().collect(),
            None => sys_locale::get_locales().filter_map(|l| l.replace('_', "-").parse().ok()).collect(),
        };
        let has = |id: &LanguageIdentifier| self.languages.iter().any(|(l, _)| l == id);
        for req in &requested {
            if has(req) {
                return req.clone();
            }
            if let Some((l, _)) = self.languages.iter().find(|(l, _)| l.language == req.language) {
                return l.clone();
            }
        }
        self.loader.fallback_language().clone()
    }

    pub fn strings(&mut self, lang: &LanguageIdentifier) -> &'static Strings {
        if let Some(s) = self.cache.get(lang) {
            return s;
        }
        let selected = self.loader.select_languages(&[lang]);
        selected.set_use_isolating(false);
        let s: &'static Strings = Box::leak(Box::new(Strings::new(selected)));
        self.cache.insert(lang.clone(), s);
        s
    }

    /// Message ids per language that are missing compared to English, and those English
    /// does not have (left over after a rename).
    #[cfg(test)]
    fn compare(&self) -> Vec<(LanguageIdentifier, Vec<String>, Vec<String>)> {
        let ids = |lang: &LanguageIdentifier| -> Vec<String> {
            self.loader.with_message_iter(lang, |it| it.map(|m| m.id.name.to_string()).collect())
        };
        let en = ids(self.loader.fallback_language());
        self.languages
            .iter()
            .map(|(lang, _)| {
                let have = ids(lang);
                let missing = en.iter().filter(|id| !have.contains(id)).cloned().collect();
                let extra = have.iter().filter(|id| !en.contains(id)).cloned().collect();
                (lang.clone(), missing, extra)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_load_and_match_english() {
        let mut i18n = I18n::load();
        let names: Vec<&str> = i18n.languages.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains(&"Deutsch") && names.contains(&"English"), "{names:?}");
        // German has every message; no language has ids English doesn't know.
        for (lang, missing, extra) in i18n.compare() {
            assert!(extra.is_empty(), "{lang}: unknown ids {extra:?}");
            if lang.language.as_str() == "de" {
                assert!(missing.is_empty(), "de: missing {missing:?}");
            }
        }
        assert_eq!(i18n.resolve(Some("de-CH")).to_string(), "de");
        assert_eq!(i18n.resolve(Some("xx")).to_string(), "en");
        let de = i18n.strings(&"de".parse().unwrap());
        assert_eq!(de.app_title, "Klavier lernen");
        assert_eq!(de.lesson_count(1), "1 Lektion");
        assert_eq!(de.lesson_count(4), "4 Lektionen");
        assert_eq!(de.part("A"), "Teil A");
        let en = i18n.strings(&"en".parse().unwrap());
        assert_eq!(en.lesson_count(1), "1 lesson");
    }
}
