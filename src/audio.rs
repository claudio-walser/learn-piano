//! SoundFont playback: a cpal output stream rendering a rustysynth synthesizer.
//!
//! The SoundFont is loaded in a background thread so the UI starts immediately.
//! Notes are sent to the audio thread through a channel.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

/// Fewer voices = less CPU. A beginner piece never needs more.
const MAX_POLYPHONY: usize = 48;
/// ~10 ms at 48 kHz: low latency without stressing slow CPUs.
const PREFERRED_BUFFER: u32 = 512;

#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    NoteOn { ch: u8, key: u8, vel: u8 },
    NoteOff { ch: u8, key: u8 },
    /// Raw channel message (e.g. sustain pedal) from a MIDI device.
    Raw { status: u8, d1: u8, d2: u8 },
    AllOff,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Loading,
    Ready,
    Failed(String),
}

type SharedSynth = Arc<Mutex<Option<Synthesizer>>>;

pub struct Audio {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    _stream: Option<cpal::Stream>,
}

impl Audio {
    pub fn start(soundfont: Option<PathBuf>) -> Audio {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(Status::Loading));
        let synth: SharedSynth = Arc::new(Mutex::new(None));

        let Some(sf_path) = soundfont else {
            *status.lock().unwrap() = Status::Failed("no SoundFont".into());
            return Audio { tx, status, _stream: None };
        };

        let stream = match open_stream(Arc::new(Mutex::new(rx)), synth.clone()) {
            Ok((stream, rate)) => {
                let st = status.clone();
                std::thread::spawn(move || {
                    let result = load_synth(&sf_path, rate);
                    *st.lock().unwrap() = match result {
                        Ok(s) => {
                            *synth.lock().unwrap() = Some(s);
                            Status::Ready
                        }
                        Err(e) => {
                            eprintln!("SoundFont: {e}");
                            Status::Failed(e)
                        }
                    };
                });
                Some(stream)
            }
            Err(e) => {
                eprintln!("Audio output: {e}");
                *status.lock().unwrap() = Status::Failed(e);
                None
            }
        };
        Audio { tx, status, _stream: stream }
    }

    pub fn sender(&self) -> Sender<Cmd> {
        self.tx.clone()
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn note_on(&self, ch: u8, key: u8, vel: u8) {
        let _ = self.tx.send(Cmd::NoteOn { ch, key, vel });
    }

    pub fn note_off(&self, ch: u8, key: u8) {
        let _ = self.tx.send(Cmd::NoteOff { ch, key });
    }

    pub fn all_off(&self) {
        let _ = self.tx.send(Cmd::AllOff);
    }
}

fn load_synth(path: &PathBuf, rate: u32) -> Result<Synthesizer, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sf = SoundFont::new(&mut BufReader::new(file)).map_err(|e| format!("{e:?}"))?;
    let mut settings = SynthesizerSettings::new(rate as i32);
    settings.maximum_polyphony = MAX_POLYPHONY;
    // Reverb/chorus cost noticeable CPU on old machines.
    settings.enable_reverb_and_chorus = false;
    Synthesizer::new(&Arc::new(sf), &settings).map_err(|e| format!("{e:?}"))
}

fn open_stream(rx: Arc<Mutex<Receiver<Cmd>>>, synth: SharedSynth) -> Result<(cpal::Stream, u32), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let rate = supported.sample_rate();
    let mut config: cpal::StreamConfig = supported.config();

    let fixed = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            Some(PREFERRED_BUFFER.clamp(*min, *max))
        }
        cpal::SupportedBufferSize::Unknown => None,
    };

    let build = |config: cpal::StreamConfig| -> Result<cpal::Stream, String> {
        let (rx, synth) = (rx.clone(), synth.clone());
        let r = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, config, rx, synth),
            cpal::SampleFormat::I16 => build::<i16>(&device, config, rx, synth),
            cpal::SampleFormat::U16 => build::<u16>(&device, config, rx, synth),
            cpal::SampleFormat::I32 => build::<i32>(&device, config, rx, synth),
            other => return Err(format!("unsupported sample format {other:?}")),
        };
        r.map_err(|e| e.to_string())
    };

    let stream = match fixed {
        Some(frames) => {
            config.buffer_size = cpal::BufferSize::Fixed(frames);
            build(config.clone()).or_else(|_| {
                config.buffer_size = cpal::BufferSize::Default;
                build(config.clone())
            })?
        }
        None => build(config)?,
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    rx: Arc<Mutex<Receiver<Cmd>>>,
    synth: SharedSynth,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut left = vec![0f32; 4096];
    let mut right = vec![0f32; 4096];

    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            let frames = data.len() / channels.max(1);
            if left.len() < frames {
                left.resize(frames, 0.0);
                right.resize(frames, 0.0);
            }
            let rx = rx.lock().unwrap();
            let mut guard = synth.try_lock();
            match guard.as_deref_mut() {
                Ok(Some(s)) => {
                    for cmd in rx.try_iter() {
                        apply(s, cmd);
                    }
                    s.render(&mut left[..frames], &mut right[..frames]);
                }
                _ => {
                    rx.try_iter().for_each(drop);
                    left[..frames].fill(0.0);
                    right[..frames].fill(0.0);
                }
            }
            for (i, frame) in data.chunks_mut(channels).enumerate() {
                for (c, sample) in frame.iter_mut().enumerate() {
                    *sample = match c {
                        0 => T::from_sample(left[i]),
                        1 => T::from_sample(right[i]),
                        _ => T::EQUILIBRIUM,
                    };
                }
            }
            if channels == 1 {
                for (i, s) in data.iter_mut().enumerate() {
                    *s = T::from_sample((left[i] + right[i]) * 0.5);
                }
            }
        },
        |e| eprintln!("audio stream error: {e}"),
        None,
    )
}

fn apply(s: &mut Synthesizer, cmd: Cmd) {
    match cmd {
        Cmd::NoteOn { ch, key, vel } => s.note_on(ch as i32, key as i32, vel as i32),
        Cmd::NoteOff { ch, key } => s.note_off(ch as i32, key as i32),
        Cmd::Raw { status, d1, d2 } => {
            s.process_midi_message((status & 0x0F) as i32, (status & 0xF0) as i32, d1 as i32, d2 as i32)
        }
        Cmd::AllOff => s.note_off_all(false),
    }
}
