use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn detect() -> Lang {
        match sys_locale::get_locale() {
            Some(l) if l.to_lowercase().starts_with("de") => Lang::De,
            _ => Lang::En,
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::De => &DE,
            Lang::En => &EN,
        }
    }
}

pub struct Strings {
    pub app_title: &'static str,
    pub choose_difficulty: &'static str,
    pub lessons: &'static str,
    pub mode_wait: &'static str,
    pub mode_realtime: &'static str,
    pub mode_listen: &'static str,
    pub hand_left: &'static str,
    pub hand_right: &'static str,
    pub hand_both: &'static str,
    pub part: &'static str,
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
    pub on: &'static str,
    pub off: &'static str,
    pub close: &'static str,
    pub no_compositions: &'static str,
    pub back: &'static str,
}

pub static DE: Strings = Strings {
    app_title: "Klavier lernen",
    choose_difficulty: "Schwierigkeitsstufe wählen",
    lessons: "Lektionen",
    mode_wait: "Warten",
    mode_realtime: "Echtzeit",
    mode_listen: "Anhören",
    hand_left: "Links",
    hand_right: "Rechts",
    hand_both: "Beide",
    part: "Teil",
    full_piece: "Ganzes Stück",
    repeat: "Wiederholen",
    next: "Weiter",
    done: "Fertig",
    hits: "Treffer",
    wrong: "Fehler",
    missed: "Verpasst",
    timing: "Timing",
    grades: ["Weiter üben!", "Nicht schlecht!", "Gut gemacht!", "Ausgezeichnet!"],
    listen_done: "Ende des Stücks",
    loading_sound: "Klang wird geladen …",
    sound_failed: "Klang nicht verfügbar",
    no_midi: "Kein MIDI-Gerät – Tasten unten antippen",
    paused: "Pausiert – zum Fortsetzen tippen",
    settings: "Einstellungen",
    language: "Sprache",
    echo: "MIDI-Eingabe über App hörbar",
    fullscreen: "Vollbild",
    on: "An",
    off: "Aus",
    close: "Schließen",
    no_compositions: "Keine Stücke gefunden",
    back: "Zurück",
};

pub static EN: Strings = Strings {
    app_title: "Learn Piano",
    choose_difficulty: "Choose difficulty",
    lessons: "lessons",
    mode_wait: "Wait",
    mode_realtime: "Real time",
    mode_listen: "Listen",
    hand_left: "Left",
    hand_right: "Right",
    hand_both: "Both",
    part: "Part",
    full_piece: "Full piece",
    repeat: "Repeat",
    next: "Next",
    done: "Done",
    hits: "Hits",
    wrong: "Wrong",
    missed: "Missed",
    timing: "Timing",
    grades: ["Keep practicing!", "Not bad!", "Well done!", "Excellent!"],
    listen_done: "End of piece",
    loading_sound: "Loading sound …",
    sound_failed: "Sound unavailable",
    no_midi: "No MIDI device – tap the keys below",
    paused: "Paused – tap to continue",
    settings: "Settings",
    language: "Language",
    echo: "Play MIDI input through the app",
    fullscreen: "Fullscreen",
    on: "On",
    off: "Off",
    close: "Close",
    no_compositions: "No pieces found",
    back: "Back",
};
