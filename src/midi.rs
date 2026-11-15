//! MIDI keyboard input with automatic (re)connection.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use midir::{Ignore, MidiInput, MidiInputConnection};

use crate::audio::Cmd;

const SCAN_INTERVAL: f64 = 2.0;

#[derive(Clone, Copy, Debug)]
pub enum MidiEvent {
    On(u8),
    Off(u8),
}

pub struct Midi {
    conn: Option<MidiInputConnection<()>>,
    pub port_name: Option<String>,
    tx: Sender<MidiEvent>,
    pub rx: Receiver<MidiEvent>,
    audio: Sender<Cmd>,
    /// When set, notes from the device are also played by the app's synthesizer.
    echo: Arc<AtomicBool>,
    last_scan: f64,
}

impl Midi {
    pub fn new(audio: Sender<Cmd>, echo: Arc<AtomicBool>) -> Midi {
        let (tx, rx) = channel();
        let mut m = Midi { conn: None, port_name: None, tx, rx, audio, echo, last_scan: f64::MIN };
        m.scan();
        m
    }

    /// Call every frame; rescans for devices every few seconds.
    pub fn poll(&mut self, now: f64) {
        if now - self.last_scan >= SCAN_INTERVAL {
            self.last_scan = now;
            self.scan();
        }
    }

    fn scan(&mut self) {
        let Ok(mut probe) = MidiInput::new("learn-piano") else { return };
        probe.ignore(Ignore::All);
        let ports = probe.ports();
        let named: Vec<_> = ports
            .iter()
            .filter_map(|p| probe.port_name(p).ok().map(|n| (p.clone(), n)))
            .collect();

        if let Some(cur) = &self.port_name {
            if named.iter().any(|(_, n)| n == cur) {
                return; // still connected
            }
            self.conn = None;
            self.port_name = None;
            let _ = self.audio.send(Cmd::AllOff);
        }

        let Some((port, name)) = named.into_iter().find(|(_, n)| {
            let l = n.to_lowercase();
            !l.contains("through") && !l.contains("learn-piano")
        }) else {
            return;
        };

        let (tx, audio, echo) = (self.tx.clone(), self.audio.clone(), self.echo.clone());
        let conn = probe.connect(
            &port,
            "learn-piano-in",
            move |_stamp, msg, _| {
                if msg.len() < 3 {
                    return;
                }
                let (status, d1, d2) = (msg[0] & 0xF0, msg[1], msg[2]);
                match status {
                    0x90 if d2 > 0 => {
                        let _ = tx.send(MidiEvent::On(d1));
                    }
                    0x80 | 0x90 => {
                        let _ = tx.send(MidiEvent::Off(d1));
                    }
                    _ => {}
                }
                if matches!(status, 0x80 | 0x90 | 0xB0) && echo.load(Ordering::Relaxed) {
                    let _ = audio.send(Cmd::Raw { status, d1, d2 });
                }
            },
            (),
        );
        match conn {
            Ok(c) => {
                self.conn = Some(c);
                self.port_name = Some(name);
            }
            Err(e) => eprintln!("MIDI connect failed: {e}"),
        }
    }
}
