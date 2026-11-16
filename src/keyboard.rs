//! On-screen piano keyboard geometry.

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

pub fn is_black(p: u8) -> bool {
    matches!(p % 12, 1 | 3 | 6 | 8 | 10)
}

/// Number of keys of the on-screen keyboard.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KeyboardSize {
    /// Whole octaves fitted to the lesson.
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "25")]
    Keys25,
    #[serde(rename = "37")]
    Keys37,
    #[serde(rename = "49")]
    Keys49,
    #[serde(rename = "61")]
    Keys61,
    #[serde(rename = "76")]
    Keys76,
    #[serde(rename = "88")]
    Keys88,
}

impl KeyboardSize {
    pub const ALL: [KeyboardSize; 7] = [
        KeyboardSize::Auto,
        KeyboardSize::Keys25,
        KeyboardSize::Keys37,
        KeyboardSize::Keys49,
        KeyboardSize::Keys61,
        KeyboardSize::Keys76,
        KeyboardSize::Keys88,
    ];

    /// Usual range of a real keyboard with this many keys (MIDI pitches).
    pub fn standard_range(self) -> Option<(u8, u8)> {
        match self {
            KeyboardSize::Auto => None,
            KeyboardSize::Keys25 => Some((48, 72)), // C3–C5
            KeyboardSize::Keys37 => Some((48, 84)), // C3–C6
            KeyboardSize::Keys49 => Some((36, 84)), // C2–C6
            KeyboardSize::Keys61 => Some((36, 96)), // C2–C7
            KeyboardSize::Keys76 => Some((28, 103)), // E1–G7
            KeyboardSize::Keys88 => Some((21, 108)), // A0–C8
        }
    }

    /// Short label for the settings ("Auto" is localized by the caller).
    pub fn label(self) -> &'static str {
        match self {
            KeyboardSize::Auto => "Auto",
            KeyboardSize::Keys25 => "25",
            KeyboardSize::Keys37 => "37",
            KeyboardSize::Keys49 => "49",
            KeyboardSize::Keys61 => "61",
            KeyboardSize::Keys76 => "76",
            KeyboardSize::Keys88 => "88",
        }
    }
}

/// Keyboard range for the given size that shows all pitches.
///
/// Fixed sizes use the standard range of such a keyboard. If the pitches don't
/// fit, the range is moved by whole octaves (like the octave buttons of small
/// keyboards); if they still don't fit, the automatic range is used.
pub fn range_for(size: KeyboardSize, pitches: impl Iterator<Item = u8>) -> (u8, u8) {
    let (mut lo, mut hi) = (u8::MAX, 0u8);
    for p in pitches {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let Some((low, high)) = size.standard_range() else {
        return auto_range(lo, hi);
    };
    if lo > hi {
        return (low, high);
    }
    let shift = if lo < low {
        -(((low - lo) as i32 + 11) / 12 * 12)
    } else if hi > high {
        ((hi - high) as i32 + 11) / 12 * 12
    } else {
        0
    };
    let (l, h) = (low as i32 + shift, high as i32 + shift);
    if l >= 0 && h <= 127 && l <= lo as i32 && h >= hi as i32 {
        (l as u8, h as u8)
    } else {
        auto_range(lo, hi)
    }
}

/// Whole octaves C..B, at least three, covering `lo..=hi`.
fn auto_range(lo: u8, hi: u8) -> (u8, u8) {
    if lo > hi {
        return (48, 83);
    }
    let mut low = lo - lo % 12;
    let mut high = (hi - hi % 12 + 11).min(127);
    while high - low + 1 < 36 {
        if (high as u32 + 12) <= 119 && (high - hi) <= (lo - low) {
            high += 12;
        } else if low >= 24 {
            low -= 12;
        } else {
            high += 12;
        }
    }
    (low, high)
}

pub struct KeyLayout {
    pub low: u8,
    pub high: u8,
    pub area: Rect,
    keys: Vec<Rect>,
}

impl KeyLayout {
    pub fn new(low: u8, high: u8, area: Rect) -> KeyLayout {
        let whites = (low..=high).filter(|&p| !is_black(p)).count().max(1);
        let white_w = area.w / whites as f32;
        let black_w = white_w * 0.6;
        let black_h = area.h * 0.62;
        let mut x = area.x;
        let keys = (low..=high)
            .map(|p| {
                if is_black(p) {
                    Rect::new(x - black_w / 2.0, area.y, black_w, black_h)
                } else {
                    let r = Rect::new(x, area.y, white_w, area.h);
                    x += white_w;
                    r
                }
            })
            .collect();
        KeyLayout { low, high, area, keys }
    }

    pub fn key(&self, p: u8) -> Option<Rect> {
        (self.low..=self.high).contains(&p).then(|| self.keys[(p - self.low) as usize])
    }

    /// Horizontal lane (x, width) for falling notes of this pitch.
    pub fn lane(&self, p: u8) -> Option<(f32, f32)> {
        let k = self.key(p)?;
        let w = if is_black(p) { k.w * 0.9 } else { k.w * 0.82 };
        Some((k.x + (k.w - w) / 2.0, w))
    }

    pub fn hit(&self, pos: Vec2) -> Option<u8> {
        if !self.area.contains(pos) {
            return None;
        }
        let range = self.low..=self.high;
        range
            .clone()
            .filter(|&p| is_black(p))
            .chain(range.filter(|&p| !is_black(p)))
            .find(|&p| self.keys[(p - self.low) as usize].contains(pos))
    }

    pub fn pitches(&self) -> std::ops::RangeInclusive<u8> {
        self.low..=self.high
    }
}

pub fn note_name(p: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[(p % 12) as usize], p as i32 / 12 - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_is_whole_octaves() {
        let (lo, hi) = range_for(KeyboardSize::Auto, [55u8, 74].into_iter());
        assert_eq!(lo % 12, 0);
        assert_eq!(hi % 12, 11);
        assert!(lo <= 55 && hi >= 74 && hi - lo + 1 >= 36);
    }

    #[test]
    fn fixed_sizes_have_their_key_count() {
        for size in KeyboardSize::ALL.into_iter().skip(1) {
            let (lo, hi) = range_for(size, [60u8, 67].into_iter());
            assert_eq!((hi - lo + 1).to_string(), size.label());
            assert!(!is_black(lo) && !is_black(hi));
        }
        assert_eq!(range_for(KeyboardSize::Keys88, [60u8].into_iter()), (21, 108));
    }

    #[test]
    fn small_keyboard_shifts_by_octaves() {
        // 25 keys C3–C5, piece in C4–C6 → one octave up
        assert_eq!(range_for(KeyboardSize::Keys25, [60u8, 84].into_iter()), (60, 84));
        // 25 keys, piece spans more than two octaves → automatic range
        assert_eq!(range_for(KeyboardSize::Keys25, [48u8, 80].into_iter()), auto_range(48, 80));
    }

    #[test]
    fn size_serializes_as_key_count() {
        assert_eq!(serde_json::to_string(&KeyboardSize::Keys88).unwrap(), "\"88\"");
        assert_eq!(serde_json::from_str::<KeyboardSize>("\"auto\"").unwrap(), KeyboardSize::Auto);
    }

    #[test]
    fn hit_prefers_black_keys() {
        let k = KeyLayout::new(60, 71, Rect::new(0.0, 0.0, 700.0, 100.0));
        assert_eq!(k.hit(vec2(100.0, 10.0)), Some(61));
        assert_eq!(k.hit(vec2(130.0, 90.0)), Some(62));
        assert_eq!(note_name(60), "C4");
    }
}
