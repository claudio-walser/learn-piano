//! Minimal MusicXML (score-partwise) reader: turns a file into timed notes.
//!
//! Supports multiple voices/staves (backup/forward), chords, ties, tempo
//! changes, repeats with voltas, and fingering. Grace and cue notes are skipped.

use std::collections::HashMap;
use std::path::Path;

use roxmltree::Node;

#[derive(Clone, Debug)]
pub struct Note {
    pub pitch: u8,
    /// Seconds at 100 % tempo.
    pub start: f64,
    pub end: f64,
    /// 1 = upper staff (right hand), 2 = lower staff (left hand).
    pub staff: u8,
    pub finger: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Score {
    pub title: Option<String>,
    /// Sorted by start time, then pitch.
    pub notes: Vec<Note>,
    /// Start time of every measure in playback order (for bar lines).
    pub measures: Vec<f64>,
    pub duration: f64,
    pub staves: u8,
}

struct RawNote {
    start: f64, // quarter notes from measure start
    dur: f64,
    pitch: u8,
    staff: u8,
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
    let mut staff_base: u8 = 0;
    let mut total_staves: u8 = 0;

    for part in root.children().filter(|n| n.has_tag_name("part")) {
        let mut divisions = 1.0;
        let mut part_staves: u8 = 1;
        let mut ending_state: Option<Vec<u32>> = None;

        for (mi, m) in part.children().filter(|n| n.has_tag_name("measure")).enumerate() {
            if measures.len() <= mi {
                measures.push(Measure::default());
            }
            let meas = &mut measures[mi];
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
                                staff: staff_base + staff.max(1),
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
        staff_base += part_staves;
        total_staves += part_staves;
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
    let mut open: HashMap<(u8, u8), usize> = HashMap::new();
    let mut ends_q: Vec<f64> = Vec::with_capacity(notes_q.len());
    for r in &notes_q {
        let key = (r.pitch, r.staff);
        if r.tie_stop {
            if let Some(&idx) = open.get(&key) {
                ends_q[idx] = ends_q[idx].max(r.start + r.dur);
                if !r.tie_start {
                    open.remove(&key);
                }
                continue;
            }
        }
        out.push(Note { pitch: r.pitch, start: r.start, end: 0.0, staff: r.staff, finger: r.finger });
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
        duration,
        staves: total_staves.max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bundled_lessons() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("compositions/oh-tannenbaum/beginner");
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
