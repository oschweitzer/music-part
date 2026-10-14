use std::io::{Read, Seek};

use oxisynth::{MidiEvent, SoundFont, Synth, SynthDescriptor};
use score_model::{Instrument, Score};

use crate::sequencer::{self, Action, Event, BEND_CENTER, BEND_RANGE_SEMITONES};

/// Silence kept after the last event so the notes can ring out.
const RELEASE_TAIL_SECS: u64 = 2;

#[derive(Debug)]
pub enum PlayerError {
    SoundFont(String),
    Synth(String),
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SoundFont(e) => write!(f, "invalid SoundFont: {e}"),
            Self::Synth(e) => write!(f, "synthesizer error: {e}"),
        }
    }
}

impl std::error::Error for PlayerError {}

/// Everything the player needs to play one score, precomputed so that handing it to a running
/// player is a cheap swap. Build it away from the real-time audio thread.
pub struct Program {
    events: Vec<Event>,
    /// Start position in samples of each event (same order as `events`).
    event_samples: Vec<u64>,
    end_sample: u64,
    tempo_bpm: u16,
    sample_rate: u32,
    /// MIDI channel and General MIDI program of each track.
    voices: Vec<(u8, u8)>,
}

impl Program {
    pub fn new(score: &Score, sample_rate: u32) -> Self {
        let events = sequencer::schedule(score);
        let event_samples: Vec<u64> = events
            .iter()
            .map(|e| sequencer::tick_to_sample(e.tick, score.tempo_bpm, sample_rate))
            .collect();
        let end_sample = event_samples
            .last()
            .map_or(0, |last| last + RELEASE_TAIL_SECS * u64::from(sample_rate));

        // One MIDI channel per track, skipping channel 9 (percussion).
        let voices = score
            .tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let channel = (i % 15) as u8;
                let channel = if channel >= 9 { channel + 1 } else { channel };
                let program = match track.instrument {
                    Instrument::Guitar => 24, // Nylon Guitar
                    Instrument::Bass => 33,   // Electric Bass (finger)
                    Instrument::Violin => 40,
                };
                (channel, program)
            })
            .collect();

        Self {
            events,
            event_samples,
            end_sample,
            tempo_bpm: score.tempo_bpm,
            sample_rate,
            voices,
        }
    }

    fn empty(sample_rate: u32) -> Self {
        Self {
            events: Vec::new(),
            event_samples: Vec::new(),
            end_sample: 0,
            tempo_bpm: 120,
            sample_rate,
            voices: Vec::new(),
        }
    }
}

/// Plays a score through a SoundFont synth. Does not touch audio hardware:
/// `render` fills an interleaved stereo buffer, so it works offline.
pub struct Player {
    synth: Synth,
    program: Program,
    next: usize,
    position: u64,
    playing: bool,
}

impl Player {
    pub fn new<R: Read + Seek>(soundfont: &mut R, sample_rate: u32) -> Result<Self, PlayerError> {
        let mut synth = Synth::new(SynthDescriptor {
            sample_rate: sample_rate as f32,
            ..Default::default()
        })
        .map_err(|e| PlayerError::Synth(format!("{e:?}")))?;
        // The SF2 parser panics on some corrupted files: turn that into an error.
        let font =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| SoundFont::load(soundfont)))
                .map_err(|_| PlayerError::SoundFont("corrupted file or not an SF2".into()))?
                .map_err(|e| PlayerError::SoundFont(format!("{e:?}")))?;
        synth.add_font(font, true);

        Ok(Self {
            synth,
            program: Program::empty(sample_rate),
            next: 0,
            position: 0,
            playing: false,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.program.sample_rate
    }

    /// Replaces the score being played. Stops and rewinds.
    pub fn load(&mut self, program: Program) {
        self.stop();
        for &(channel, program_id) in &program.voices {
            let _ = self.synth.send_event(MidiEvent::ProgramChange {
                channel,
                program_id,
            });
            // Registered parameter 0 is the pitch wheel range, in semitones.
            for (ctrl, value) in [(101, 0), (100, 0), (6, BEND_RANGE_SEMITONES as u8)] {
                let _ = self.synth.send_event(MidiEvent::ControlChange {
                    channel,
                    ctrl,
                    value,
                });
            }
        }
        self.program = program;
    }

    /// Convenience for offline use: builds the `Program` for the player's sample rate.
    pub fn load_score(&mut self, score: &Score) {
        self.load(Program::new(score, self.program.sample_rate));
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
        self.silence();
    }

    /// Stops and rewinds to the start.
    pub fn stop(&mut self) {
        self.pause();
        self.next = 0;
        self.position = 0;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Playback position in samples.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Playback position in 1/128 of a whole note, to place a cursor on the score.
    pub fn position_ticks(&self) -> f64 {
        sequencer::sample_to_tick(
            self.position,
            self.program.tempo_bpm,
            self.program.sample_rate,
        )
    }

    fn silence(&mut self) {
        for &(channel, _) in &self.program.voices {
            let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
            let _ = self.synth.send_event(MidiEvent::PitchBend {
                channel,
                value: BEND_CENTER,
            });
        }
    }

    /// Fills `out` (interleaved stereo L,R,L,R...). Writes silence when paused or finished.
    /// When the score and its release tail are over, the player stops and rewinds by itself.
    pub fn render(&mut self, out: &mut [f32]) {
        let (frames, _) = out.as_chunks_mut::<2>();
        for frame in frames {
            if !self.playing {
                frame.fill(0.0);
                continue;
            }
            while self.next < self.program.events.len()
                && self.program.event_samples[self.next] <= self.position
            {
                self.dispatch(self.program.events[self.next]);
                self.next += 1;
            }
            let (l, r) = self.synth.read_next();
            frame[0] = l;
            frame[1] = r;
            self.position += 1;
            if self.position >= self.program.end_sample {
                self.stop();
            }
        }
    }

    fn dispatch(&mut self, e: Event) {
        let Some(&(channel, _)) = self.program.voices.get(e.track) else {
            return;
        };
        let event = match e.action {
            Action::NoteOn { pitch, velocity } => MidiEvent::NoteOn {
                channel,
                key: pitch,
                vel: velocity,
            },
            Action::NoteOff { pitch } => MidiEvent::NoteOff {
                channel,
                key: pitch,
            },
            Action::PitchBend(value) => MidiEvent::PitchBend { channel, value },
        };
        // An out-of-range note must not interrupt playback.
        let _ = self.synth.send_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::*;
    use std::fs::File;

    fn one_note_score() -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            tracks: vec![Track {
                name: "g".into(),
                instrument: Instrument::Guitar,
                tuning: GUITAR_STANDARD.to_vec(),
                measures: vec![Measure {
                    time_signature: TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    },
                    voices: vec![Voice {
                        beats: vec![Beat {
                            duration: Duration {
                                value: NoteValue::Whole,
                                dots: 0,
                            },
                            notes: vec![Note {
                                string: 1,
                                fret: 0,
                                technique: None,
                                tied: false,
                            }],
                        }],
                    }],
                }],
            }],
        }
    }

    #[test]
    fn invalid_soundfont_is_an_error() {
        let mut junk = std::io::Cursor::new(vec![0u8; 64]);
        assert!(matches!(
            Player::new(&mut junk, 44_100),
            Err(PlayerError::SoundFont(_))
        ));
    }

    /// Path of the SoundFont used by audible-path tests: `$SOUNDFONT`, or the one fetched by
    /// `pnpm fetch:soundfont`. `None` skips those tests.
    fn test_soundfont() -> Option<std::path::PathBuf> {
        // An empty variable counts as unset.
        if let Some(path) = std::env::var_os("SOUNDFONT").filter(|p| !p.is_empty()) {
            return Some(path.into());
        }
        let default = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/soundfonts/GeneralUser-GS.sf2");
        default.exists().then_some(default)
    }

    #[test]
    fn renders_sound_only_while_playing() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&one_note_score());
        let mut buf = vec![0.0f32; 2 * 4410];

        player.render(&mut buf);
        assert!(
            buf.iter().all(|s| *s == 0.0),
            "silence expected before play()"
        );

        player.play();
        player.render(&mut buf);
        player.render(&mut buf);
        assert!(
            buf.iter().any(|s| s.abs() > 1e-4),
            "sound expected after play()"
        );

        player.pause();
        player.render(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.0));
        assert!(player.position() > 0);
        player.stop();
        assert_eq!(player.position(), 0);
    }

    #[test]
    fn every_demo_track_is_audible() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let demo = score_model::demo_score();

        for track in &demo.tracks {
            let score = Score {
                tracks: vec![track.clone()],
                ..demo.clone()
            };
            let mut player = Player::new(&mut File::open(&path).unwrap(), 44_100).unwrap();
            player.load_score(&score);
            player.play();

            let mut buf = vec![0.0f32; 2 * 44_100];
            player.render(&mut buf);
            let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.01, "{} is nearly silent (peak {peak})", track.name);
            assert!(peak <= 1.0, "{} clips (peak {peak})", track.name);
        }
    }

    #[test]
    fn stops_and_rewinds_when_the_score_is_over() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&one_note_score());
        player.play();

        // A whole note at 120 bpm lasts 2 s (4 quarters of 0.5 s), plus a 2 s release tail.
        let mut buf = vec![0.0f32; 2 * 44_100];
        player.render(&mut buf);
        assert!(player.is_playing());
        // After 1 s at 120 bpm: 2 quarter notes = 64 ticks.
        assert!((player.position_ticks() - 64.0).abs() < 0.5);

        for _ in 0..4 {
            player.render(&mut buf);
        }
        assert!(!player.is_playing());
        assert_eq!(player.position(), 0);
    }

    #[test]
    fn loading_a_new_score_rewinds() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&one_note_score());
        player.play();
        player.render(&mut vec![0.0f32; 2 * 1000]);
        assert!(player.position() > 0);

        player.load_score(&one_note_score());
        assert_eq!(player.position(), 0);
        assert!(!player.is_playing());
    }

    /// Two half notes, the first one optionally a slide up to the second.
    fn two_notes(technique: Option<Technique>) -> Score {
        let mut score = one_note_score();
        let half = |fret, technique| Beat {
            duration: Duration {
                value: NoteValue::Half,
                dots: 0,
            },
            notes: vec![Note {
                string: 1,
                fret,
                technique,
                tied: false,
            }],
        };
        score.tracks[0].measures[0].voices[0].beats = vec![half(0, technique), half(5, None)];
        score
    }

    fn render_seconds(path: &std::path::Path, score: &Score, seconds: usize) -> Vec<f32> {
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(score);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100 * seconds];
        player.render(&mut buf);
        buf
    }

    #[test]
    fn a_slide_changes_the_sound_only_once_the_glide_starts() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let plain = render_seconds(&path, &two_notes(None), 2);
        let slid = render_seconds(&path, &two_notes(Some(Technique::Slide)), 2);

        // At 120 bpm the first half note lasts 1 s and the glide covers its second half.
        let before_glide = 2 * 22_050;
        assert_eq!(plain[..before_glide], slid[..before_glide]);
        assert!(
            plain[before_glide..2 * 44_100]
                .iter()
                .zip(&slid[before_glide..2 * 44_100])
                .any(|(a, b)| (a - b).abs() > 1e-3),
            "the glide should be audible"
        );
        assert!(slid.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}
