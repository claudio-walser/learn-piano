//! On-screen piano keyboard geometry.

use macroquad::prelude::*;

pub fn is_black(p: u8) -> bool {
    matches!(p % 12, 1 | 3 | 6 | 8 | 10)
}

/// Keyboard range (whole octaves C..B, at least three) covering all pitches.
pub fn range_for(pitches: impl Iterator<Item = u8>) -> (u8, u8) {
    let (mut lo, mut hi) = (u8::MAX, 0u8);
    for p in pitches {
        lo = lo.min(p);
        hi = hi.max(p);
    }
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
        let (lo, hi) = range_for([55u8, 74].into_iter());
        assert_eq!(lo % 12, 0);
        assert_eq!(hi % 12, 11);
        assert!(lo <= 55 && hi >= 74 && hi - lo + 1 >= 36);
    }

    #[test]
    fn hit_prefers_black_keys() {
        let k = KeyLayout::new(60, 71, Rect::new(0.0, 0.0, 700.0, 100.0));
        assert_eq!(k.hit(vec2(100.0, 10.0)), Some(61));
        assert_eq!(k.hit(vec2(130.0, 90.0)), Some(62));
        assert_eq!(note_name(60), "C4");
    }
}
