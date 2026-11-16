//! Minimal MusicXML (score-partwise) reader: turns a file into timed notes.
//!
//! Supports multiple voices/staves (backup/forward), chords, ties, tempo
//! changes, repeats with voltas, and fingering. Grace and cue notes are skipped.
//! Several `<part>`s make a duet: part 0 is the student, part 1 the teacher.

use std::collections::HashMap;
use std::path::Path;

use roxmltree::Node;

#[derive(Clone, Debug)]
pub struct Note {
    pub pitch: u8,
    /// Seconds at 100 % tempo.
    pub start: f64,
    pub end: f64,
    /// Staff within its part: 1 = upper staff (right hand), 2 = lower staff (left hand).
    pub staff: u8,
    pub finger: Option<u8>,
    /// Index of the `<part>` (0 = first). More than one part = duet.
    pub part: u8,
}

#[derive(Clone, Debug)]
pub struct Score {
    pub title: Option<String>,
    /// Sorted by start time, then pitch.
    pub notes: Vec<Note>,
    /// Start time of every measure in playback order (for bar lines).
    pub measures: Vec<f64>,
    /// Printed number of each entry in `measures` (repeats show the same number again).
    pub measure_numbers: Vec<String>,
    pub duration: f64,
    /// Staves per part (the most of any part).
    pub staves: u8,
    /// Part names from the part list, in order.
    pub parts: Vec<String>,
}

impl Score {
    pub fn is_duet(&self) -> bool {
        self.parts.len() > 1
    }
}

struct RawNote {
    start: f64, // quarter notes from measure start
    dur: f64,
    pitch: u8,
    staff: u8,
    part: u8,
    finger: Option<u8>,
    tie_start: bool,
    tie_stop: bool,
}

#[derive(Default)]
struct Measure {
    notes: Vec<RawNote>,
    len: f64,
    tempos: Vec<(f64, f64)>,
    repeat_forward: bool,
    repeat_backward: Option<u32>,
    ending: Option<Vec<u32>>,
    number: String,
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn child_text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

fn child_num(n: Node, name: &str) -> Option<f64> {
    child_text(n, name)?.parse().ok()
}

fn parse_pitch(p: Node) -> Option<u8> {
    let step = match child_text(p, "step")? {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let alter = child_num(p, "alter").unwrap_or(0.0).round() as i32;
    let octave: i32 = child_text(p, "octave")?.parse().ok()?;
    let midi = (octave + 1) * 12 + step + alter;
    (0..=127).contains(&midi).then_some(midi as u8)
}

fn metronome_bpm(dir: Node) -> Option<f64> {
    let m = dir.descendants().find(|n| n.has_tag_name("metronome"))?;
    let bpm = child_num(m, "per-minute")?;
    let unit = match child_text(m, "beat-unit")? {
        "whole" => 4.0,
        "half" => 2.0,
        "eighth" => 0.5,
        "16th" => 0.25,
        _ => 1.0,
    };
    let dotted = if child(m, "beat-unit-dot").is_some() { 1.5 } else { 1.0 };
    Some(bpm * unit * dotted)
}

fn parse_ending_numbers(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect()
}

pub fn load(path: &Path) -> Result<Score, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn parse(text: &str) -> Result<Score, String> {
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(text, opts).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if !root.has_tag_name("score-partwise") {
        return Err("only score-partwise MusicXML is supported".into());
    }
    let title = root
        .descendants()
        .find(|n| n.has_tag_name("work-title") || n.has_tag_name("movement-title"))
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string());

    let mut measures: Vec<Measure> = Vec::new();
    let mut max_staves: u8 = 0;
    let mut parts: Vec<String> = Vec::new();

    for (pi, part) in root.children().filter(|n| n.has_tag_name("part")).enumerate() {
        let id = part.attribute("id").unwrap_or("");
        let name = root
            .descendants()
            .find(|n| n.has_tag_name("score-part") && n.attribute("id") == Some(id))
            .and_then(|n| child_text(n, "part-name"))
            .unwrap_or(id);
        parts.push(name.to_string());
        let mut divisions = 1.0;
        let mut part_staves: u8 = 1;
        let mut ending_state: Option<Vec<u32>> = None;

        for (mi, m) in part.children().filter(|n| n.has_tag_name("measure")).enumerate() {
            if measures.len() <= mi {
                measures.push(Measure::default());
            }
            let meas = &mut measures[mi];
            if meas.number.is_empty() {
                meas.number = m.attribute("number").unwrap_or("").to_string();
            }
            let mut cursor = 0.0_f64;
            let mut max_cursor = 0.0_f64;
            let mut last_start = 0.0_f64;
            let mut cur_ending = ending_state.clone();

            for el in m.children().filter(|n| n.is_element()) {
                match el.tag_name().name() {
                    "attributes" => {
                        if let Some(d) = child_num(el, "divisions") {
                            if d > 0.0 {
                                divisions = d;
                            }
                        }
                        if let Some(s) = child_num(el, "staves") {
                            part_staves = s as u8;
                        }
                    }
                    "note" => {
                        if child(el, "grace").is_some() || child(el, "cue").is_some() {
                            continue;
                        }
                        let dur = child_num(el, "duration").unwrap_or(0.0) / divisions;
                        let is_chord = child(el, "chord").is_some();
                        let start = if is_chord { last_start } else { cursor };
                        if let Some(p) = child(el, "pitch").and_then(parse_pitch) {
                            let staff = child_num(el, "staff").unwrap_or(1.0) as u8;
                            let finger = el
                                .descendants()
                                .find(|n| n.has_tag_name("fingering"))
                                .and_then(|n| n.text())
                                .and_then(|t| t.trim().chars().next())
                                .and_then(|c| c.to_digit(10))
                                .map(|d| d as u8);
                            let ties: Vec<&str> = el
                                .children()
                                .filter(|n| n.has_tag_name("tie"))
                                .filter_map(|n| n.attribute("type"))
                                .collect();
                            meas.notes.push(RawNote {
                                start,
                                dur,
                                pitch: p,
                                staff: staff.max(1),
                                part: pi as u8,
                                finger,
                                tie_start: ties.contains(&"start"),
                                tie_stop: ties.contains(&"stop"),
                            });
                        }
                        if !is_chord {
                            last_start = cursor;
                            cursor += dur;
                        }
                        max_cursor = max_cursor.max(cursor).max(start + dur);
                    }
                    "backup" => cursor -= child_num(el, "duration").unwrap_or(0.0) / divisions,
                    "forward" => {
                        cursor += child_num(el, "duration").unwrap_or(0.0) / divisions;
                        max_cursor = max_cursor.max(cursor);
                    }
                    "direction" => {
                        let bpm = child(el, "sound")
                            .and_then(|s| s.attribute("tempo"))
                            .and_then(|t| t.parse().ok())
                            .or_else(|| metronome_bpm(el));
                        if let Some(bpm) = bpm {
                            meas.tempos.push((cursor, bpm));
                        }
                    }
                    "sound" => {
                        if let Some(bpm) = el.attribute("tempo").and_then(|t| t.parse().ok()) {
                            meas.tempos.push((cursor, bpm));
                        }
                    }
                    "barline" => {
                        if let Some(r) = child(el, "repeat") {
                            match r.attribute("direction") {
                                Some("forward") => meas.repeat_forward = true,
                                Some("backward") => {
                                    let times = r.attribute("times").and_then(|t| t.parse().ok());
                                    meas.repeat_backward = Some(times.unwrap_or(2));
                                }
                                _ => {}
                            }
                        }
                        if let Some(e) = child(el, "ending") {
                            match e.attribute("type") {
                                Some("start") => {
                                    let nums = parse_ending_numbers(e.attribute("number").unwrap_or("1"));
                                    cur_ending = Some(nums.clone());
                                    ending_state = Some(nums);
                                }
                                Some("stop") | Some("discontinue") => ending_state = None,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            meas.len = meas.len.max(max_cursor);
            if cur_ending.is_some() {
                meas.ending = cur_ending;
            }
        }
        max_staves = max_staves.max(part_staves);
    }

    // Playback order with repeats unrolled.
    let n = measures.len();
    let mut order = Vec::new();
    let mut repeat_counts = vec![0u32; n];
    let (mut i, mut start, mut pass) = (0usize, 0usize, 1u32);
    while i < n && order.len() < 100_000 {
        let m = &measures[i];
        if m.repeat_forward && start != i {
            start = i;
            pass = 1;
        }
        if let Some(e) = &m.ending {
            if !e.contains(&pass) {
                i += 1;
                continue;
            }
        }
        order.push(i);
        if let Some(times) = m.repeat_backward {
            if repeat_counts[i] + 1 < times {
                repeat_counts[i] += 1;
                pass += 1;
                i = start;
                continue;
            }
            start = i + 1;
            pass = 1;
        }
        i += 1;
    }

    // Absolute positions in quarter notes.
    let mut tempo_map: Vec<(f64, f64)> = Vec::new();
    let mut measure_q = Vec::with_capacity(order.len());
    let mut notes_q: Vec<RawNote> = Vec::new();
    let mut pos = 0.0;
    for &mi in &order {
        let m = &measures[mi];
        measure_q.push(pos);
        tempo_map.extend(m.tempos.iter().map(|&(o, bpm)| (pos + o, bpm)));
        notes_q.extend(m.notes.iter().map(|r| RawNote { start: pos + r.start, ..*r }));
        pos += m.len;
    }
    let end_q = pos;

    // Tempo map: quarter position -> seconds.
    tempo_map.sort_by(|a, b| a.0.total_cmp(&b.0));
    let initial = tempo_map.first().map(|t| t.1).unwrap_or(100.0);
    if tempo_map.first().map_or(true, |t| t.0 > 0.0) {
        tempo_map.insert(0, (0.0, initial));
    }
    let mut seg_secs = Vec::with_capacity(tempo_map.len());
    let mut acc = 0.0;
    for (k, &(q, _)) in tempo_map.iter().enumerate() {
        if k > 0 {
            let (pq, pbpm) = tempo_map[k - 1];
            acc += (q - pq) * 60.0 / pbpm;
        }
        seg_secs.push(acc);
    }
    let to_sec = |q: f64| -> f64 {
        let k = tempo_map.partition_point(|t| t.0 <= q).saturating_sub(1);
        let (sq, bpm) = tempo_map[k];
        seg_secs[k] + (q - sq) * 60.0 / bpm.max(1.0)
    };

    // Merge ties and convert to seconds.
    notes_q.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    let mut out: Vec<Note> = Vec::with_capacity(notes_q.len());
    let mut open: HashMap<(u8, u8, u8), usize> = HashMap::new();
    let mut ends_q: Vec<f64> = Vec::with_capacity(notes_q.len());
    for r in &notes_q {
        let key = (r.pitch, r.part, r.staff);
        if r.tie_stop {
            if let Some(&idx) = open.get(&key) {
                ends_q[idx] = ends_q[idx].max(r.start + r.dur);
                if !r.tie_start {
                    open.remove(&key);
                }
                continue;
            }
        }
        out.push(Note { pitch: r.pitch, start: r.start, end: 0.0, staff: r.staff, finger: r.finger, part: r.part });
        ends_q.push(r.start + r.dur);
        if r.tie_start {
            open.insert(key, out.len() - 1);
        }
    }
    for (note, end) in out.iter_mut().zip(ends_q) {
        let s = to_sec(note.start);
        note.start = s;
        note.end = to_sec(end).max(s + 0.05);
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));

    let duration = out.iter().map(|n| n.end).fold(to_sec(end_q), f64::max);
    Ok(Score {
        title,
        notes: out,
        measures: measure_q.into_iter().map(to_sec).collect(),
        measure_numbers: order.iter().map(|&mi| measures[mi].number.clone()).collect(),
        duration,
        staves: max_staves.max(1),
        parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bundled_lessons() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/oh-christmas-tree/beginner");
        for f in ["01-part-A.musicxml", "02-part-B.musicxml", "full.musicxml"] {
            let s = load(&root.join(f)).unwrap();
            assert!(!s.notes.is_empty(), "{f}");
            assert_eq!(s.staves, 2);
            assert!(s.notes.iter().any(|n| n.finger.is_some()));
            assert!(s.notes.windows(2).all(|w| w[0].start <= w[1].start));
        }
        // full.musicxml ends with a backward repeat: the piece is played twice.
        let a = load(&root.join("01-part-A.musicxml")).unwrap();
        let full = load(&root.join("full.musicxml")).unwrap();
        assert!(full.notes.len() > a.notes.len() * 2);
    }

    #[test]
    fn parses_silent_night_beginner() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/silent-night/beginner");
        let full = load(&root.join("full.musicxml")).unwrap();
        // 20 measures, bars 9–12 repeated: 24 bars of 3/4 at ♩ = 80.
        assert_eq!(full.measures.len(), 24);
        assert!((full.duration - 24.0 * 3.0 * 0.75).abs() < 1e-6);
        let b = load(&root.join("02-part-B.musicxml")).unwrap();
        assert_eq!(b.measures.len(), 8);
        // B flats in both hands, final chord one octave up (8va): F5 + A5.
        assert!(full.notes.iter().any(|n| n.pitch == 70 && n.staff == 1));
        assert!(full.notes.iter().any(|n| n.pitch == 58 && n.staff == 2));
        let last: Vec<u8> = full.notes.iter().filter(|n| n.start == full.notes.last().unwrap().start && n.staff == 1).map(|n| n.pitch).collect();
        assert_eq!(last, [77, 81]);
        assert!(full.notes.iter().all(|n| n.finger.is_some()));
        for f in ["01-part-A.musicxml", "03-part-C.musicxml"] {
            assert!(!load(&root.join(f)).unwrap().notes.is_empty(), "{f}");
        }
    }

    #[test]
    fn parses_happy_birthday_with_pickup() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/happy-birthday/beginner/full.musicxml");
        let s = load(&p).unwrap();
        // One-beat pickup + 8 bars of 3/4 at ♩ = 112.
        assert_eq!(s.measures.len(), 9);
        let beat = 60.0 / 112.0;
        assert!((s.duration - 25.0 * beat).abs() < 1e-6);
        assert_eq!(s.notes[0].pitch, 55); // G3 on the pickup, left hand
        assert_eq!(s.notes[0].staff, 2);
        assert!((s.measures[1] - beat).abs() < 1e-6);
    }

    #[test]
    fn parses_fuer_elise_beginner() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/fuer-elise/beginner/full.musicxml");
        let s = load(&p).unwrap();
        // 21 bars of 3/4 at ♩ = 72, starting with E5–D#5 after a half rest.
        assert_eq!(s.measures.len(), 21);
        assert!((s.duration - 63.0 * 60.0 / 72.0).abs() < 1e-6);
        assert_eq!((s.notes[0].pitch, s.notes[1].pitch), (76, 75));
        assert_eq!(s.notes.iter().map(|n| n.pitch).min(), Some(45)); // A2
        assert!(s.notes.iter().all(|n| n.finger.is_some()));
    }

    #[test]
    fn parses_ode_to_joy_beginner() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/ode-to-joy/beginner/full.musicxml");
        let s = load(&p).unwrap();
        // 16 bars of 4/4 at ♩ = 96; melody E E F G over a C chord (C3 E3 G3).
        assert_eq!(s.measures.len(), 16);
        assert!((s.duration - 64.0 * 60.0 / 96.0).abs() < 1e-6);
        let first: Vec<u8> = s.notes.iter().filter(|n| n.start < 1e-6).map(|n| n.pitch).collect();
        assert_eq!(first.len(), 4);
        assert!(first.contains(&64) && first.contains(&48));
        assert_eq!(s.notes.iter().map(|n| n.pitch).min(), Some(47)); // B2
        assert!(s.notes.iter().all(|n| n.finger.is_some()));
    }

    #[test]
    fn parses_all_my_little_ducklings_levels() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/all-my-little-ducklings");
        // Same melody (C D E F G G A A A A G …) in all three levels, only the left hand differs.
        for (level, lh_notes) in [("prep", 9), ("beginner", 13), ("intermediate", 15)] {
            let s = load(&dir.join(level).join("full.musicxml")).unwrap();
            assert_eq!(s.measures.len(), 10, "{level}");
            assert!((s.duration - 40.0 * 60.0 / 100.0).abs() < 1e-6, "{level}");
            let rh: Vec<u8> = s.notes.iter().filter(|n| n.staff == 1).map(|n| n.pitch).collect();
            assert_eq!(rh.len(), 27, "{level}");
            assert_eq!(rh[..7], [60, 62, 64, 65, 67, 67, 69], "{level}");
            assert_eq!(s.notes.iter().filter(|n| n.staff == 2).count(), lh_notes, "{level}");
            assert!(s.notes.iter().all(|n| n.finger.is_some()), "{level}");
        }
    }

    #[test]
    fn parses_duet_parts_and_voltas() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/happy-birthday/duet/full.musicxml");
        let s = load(&p).unwrap();
        assert!(s.is_duet());
        assert_eq!(s.parts, ["Student", "Teacher"]);
        assert_eq!(s.staves, 2);
        // Pickup, bars 1–2, 3–10 (1st ending), again 3–9, 11 (2nd ending), 12.
        assert_eq!(s.measures.len(), 1 + 2 + 8 + 7 + 2);
        let nums: Vec<&str> = s.measure_numbers.iter().map(String::as_str).collect();
        assert_eq!(nums[..12], ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "3"]);
        assert_eq!(nums[17..], ["9", "11", "12"]);
        // Student two octaves up (G6 = 91), teacher in the middle; staves are per part.
        assert_eq!(s.notes.iter().filter(|n| n.part == 0).map(|n| n.pitch).max(), Some(91));
        assert!(s.notes.iter().filter(|n| n.part == 1).all(|n| n.pitch < 72));
        assert!(s.notes.iter().all(|n| n.staff == 1 || n.staff == 2));
        // "And many more!" ends on the student's Bb5.
        assert!(s.notes.iter().any(|n| n.part == 0 && n.pitch == 82));
        assert!(s.notes.iter().all(|n| n.finger.is_some()), "every note of both parts has a finger");
    }

    #[test]
    fn parses_silent_night_prep_and_intermediate() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/silent-night");
        let prep = load(&dir.join("prep/full.musicxml")).unwrap();
        assert_eq!(prep.measures.len(), 24);
        // Tied F3 in bars 19–20 is one note of two bars.
        let f3: Vec<_> = prep.notes.iter().filter(|n| n.pitch == 53).collect();
        assert_eq!(f3.len(), 1);
        assert!((f3[0].end - f3[0].start - 2.0 * 3.0 * 0.75).abs() < 1e-6);

        // 6/8 with dotted rhythms: 12 bars of 6 eighths at ♩ = 80.
        let im = load(&dir.join("intermediate/full.musicxml")).unwrap();
        assert_eq!(im.measures.len(), 12);
        assert!((im.duration - 12.0 * 3.0 * 0.75).abs() < 1e-6);
        assert!(im.notes.iter().any(|n| n.pitch == 56)); // G#3
        assert!(im.notes.iter().any(|n| n.pitch == 58)); // Bb3
        assert!(im.notes.iter().filter(|n| n.staff == 1).all(|n| n.finger.is_some()));
        for f in ["01-part-A", "02-part-B", "03-part-C"] {
            let s = load(&dir.join(format!("intermediate/{f}.musicxml"))).unwrap();
            assert_eq!(s.measures.len(), 4, "{f}");
        }
    }

    #[test]
    fn repeats_ties_and_chords() {
        let xml = r#"<score-partwise><part id="P1">
          <measure number="1"><attributes><divisions>1</divisions></attributes>
            <direction><sound tempo="60"/></direction>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><tie type="start"/></note>
            <note><chord/><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration></note>
            <barline location="right"><repeat direction="backward"/></barline></measure>
          <measure number="2">
            <note><pitch><step>C</step><alter>1</alter><octave>4</octave></pitch><duration>1</duration></note>
          </measure></part></score-partwise>"#;
        let s = parse(xml).unwrap();
        let p: Vec<(u8, f64)> = s.notes.iter().map(|n| (n.pitch, n.start)).collect();
        assert_eq!(p, vec![(60, 0.0), (64, 0.0), (60, 2.0), (64, 2.0), (61, 4.0)]);
        assert_eq!(s.duration, 5.0);
    }
}
