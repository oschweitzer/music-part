use std::io::{Read, Seek};
use std::ops::RangeInclusive;

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

/// The most tracks a score can have (see `Score::validate`); each gets its own channels.
const MAX_TRACKS: usize = 64;
/// Where the fallback channels start: after every track channel. Channel 9 (percussion) is skipped.
const FALLBACK_BASE: usize = MAX_TRACKS + 6;
const SYNTH_GAIN: f32 = 0.6;
/// The synthesizer wants a multiple of 16.
const SYNTH_CHANNELS: u8 = ((FALLBACK_BASE + MAX_TRACKS).div_ceil(16) * 16) as u8;

/// The MIDI channels of a track, and what it plays.
struct Channel {
    channel: u8,
    /// Plays the notes a sample bank has no recording for, with the General MIDI sound.
    fallback: u8,
    /// General MIDI program, used when no sample bank is set for the instrument.
    program: u8,
    instrument: Instrument,
}

impl Channel {
    fn both(&self) -> [u8; 2] {
        [self.channel, self.fallback]
    }
}

/// A SoundFont that replaces the General MIDI sound of one instrument.
struct Bank {
    instrument: Instrument,
    /// Bank number the SoundFont answers to.
    number: u8,
    /// Keys the recordings cover; notes outside are played with the General MIDI sound.
    keys: RangeInclusive<u8>,
}

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
    voices: Vec<Channel>,
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

        // Two MIDI channels per track (the synthesizer is given enough of them), skipping the
        // percussion channel 9.
        let voices = score
            .tracks
            .iter()
            .take(MAX_TRACKS)
            .enumerate()
            .map(|(i, track)| {
                let channel = (i + usize::from(i >= 9)) as u8;
                Channel {
                    channel,
                    fallback: (FALLBACK_BASE + i) as u8,
                    program: track.instrument.midi_program(),
                    instrument: track.instrument,
                }
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

/// Bank Select (most significant byte) controller.
const BANK_SELECT_MSB: u8 = 0;
/// Sample banks a player can hold. Bank numbers stay well under the 128 a controller can carry.
const MAX_BANKS: u8 = 32;

/// The SF2 parser panics on some corrupted files: turn that into an error.
fn read_soundfont<R: Read + Seek>(soundfont: &mut R) -> Result<SoundFont, PlayerError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| SoundFont::load(soundfont)))
        .map_err(|_| PlayerError::SoundFont("corrupted file or not an SF2".into()))?
        .map_err(|e| PlayerError::SoundFont(format!("{e:?}")))
}

/// Plays a score through a SoundFont synth. Does not touch audio hardware:
/// `render` fills an interleaved stereo buffer, so it works offline.
pub struct Player {
    synth: Synth,
    program: Program,
    next: usize,
    position: u64,
    playing: bool,
    /// Which tracks make sound, by track index; a track past the end of the list does.
    audible: Vec<bool>,
    /// Sample banks: the bank number each instrument's SoundFont answers to.
    banks: Vec<Bank>,
}

impl Player {
    pub fn new<R: Read + Seek>(soundfont: &mut R, sample_rate: u32) -> Result<Self, PlayerError> {
        let mut synth = Synth::new(SynthDescriptor {
            sample_rate: sample_rate as f32,
            midi_channels: SYNTH_CHANNELS,
            // The default (0.2) leaves a piece quiet; this keeps a dense chord well under clipping.
            gain: SYNTH_GAIN,
            ..Default::default()
        })
        .map_err(|e| PlayerError::Synth(format!("{e:?}")))?;
        synth.add_font(read_soundfont(soundfont)?, true);

        Ok(Self {
            synth,
            program: Program::empty(sample_rate),
            next: 0,
            position: 0,
            playing: false,
            audible: Vec::new(),
            banks: Vec::new(),
        })
    }

    /// Plays the tracks of `instrument` with the sounds of another SoundFont (a sample bank made
    /// of one instrument, as its first preset), instead of the General MIDI sound of the base
    /// one. `keys` are the MIDI keys the recordings cover: a note outside them is played with the
    /// General MIDI sound rather than left silent. Call before loading a score. A second bank for
    /// the same instrument replaces the first.
    pub fn add_bank<R: Read + Seek>(
        &mut self,
        instrument: Instrument,
        soundfont: &mut R,
        keys: RangeInclusive<u8>,
    ) -> Result<(), PlayerError> {
        let font = read_soundfont(soundfont)?;
        // Bank 0 is the base SoundFont; each bank takes the next number, shifting its presets.
        let number = u8::try_from(self.banks.len() + 1)
            .ok()
            .filter(|b| *b <= MAX_BANKS)
            .ok_or_else(|| PlayerError::SoundFont("too many sample banks".into()))?;
        let id = self.synth.add_font(font, false);
        self.synth.set_bank_offset(id, u32::from(number));
        self.banks.push(Bank {
            instrument,
            number,
            keys,
        });
        Ok(())
    }

    fn bank_of(&self, instrument: Instrument) -> Option<&Bank> {
        self.banks.iter().rev().find(|b| b.instrument == instrument)
    }

    pub fn sample_rate(&self) -> u32 {
        self.program.sample_rate
    }

    /// Replaces the score being played. Stops and rewinds.
    pub fn load(&mut self, program: Program) {
        self.stop();
        for voice in &program.voices {
            // A sample bank for the instrument sits at its own bank number, with the sound at
            // program 0; the fallback channel and instruments without a bank use the General
            // MIDI sound of the base SoundFont.
            let bank = self.bank_of(voice.instrument).map(|b| b.number);
            self.set_sound(
                voice.channel,
                bank.unwrap_or(0),
                bank.map_or(voice.program, |_| 0),
            );
            self.set_sound(voice.fallback, 0, voice.program);
        }
        self.program = program;
    }

    fn set_sound(&mut self, channel: u8, bank: u8, program_id: u8) {
        let _ = self.synth.send_event(MidiEvent::ControlChange {
            channel,
            ctrl: BANK_SELECT_MSB,
            value: bank,
        });
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

    /// Moves to a position in 1/128 of a whole note, playing or not. Notes already sounding at
    /// that point are not started: playback picks up with the next note.
    pub fn seek(&mut self, ticks: f64) {
        // Cut the sound, not just release it: the notes of the old position must not ring on.
        for voice in &self.program.voices {
            for channel in voice.both() {
                let _ = self.synth.send_event(MidiEvent::AllSoundOff { channel });
            }
        }
        self.silence();
        let ticks = if ticks.is_finite() {
            ticks.max(0.0)
        } else {
            0.0
        };
        let wanted = sequencer::tick_to_sample(
            ticks.min(f64::from(u32::MAX)).round() as u32,
            self.program.tempo_bpm,
            self.program.sample_rate,
        );
        // Stay inside the score so the end-of-score logic still stops the player.
        self.position = wanted.min(self.program.end_sample.saturating_sub(1));
        self.next = self
            .program
            .event_samples
            .partition_point(|&sample| sample < self.position);
    }

    /// Chooses the tracks that make sound (mute and solo are worked out by the caller). Silences
    /// the ones that stop now; the others start with their next note.
    pub fn set_audible(&mut self, audible: Vec<bool>) {
        self.audible = audible;
        for (track, voice) in self.program.voices.iter().enumerate() {
            if !self.is_audible(track) {
                for channel in voice.both() {
                    let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
                }
            }
        }
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
        for voice in &self.program.voices {
            for channel in voice.both() {
                let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
                let _ = self.synth.send_event(MidiEvent::PitchBend {
                    channel,
                    value: BEND_CENTER,
                });
            }
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

    fn is_audible(&self, track: usize) -> bool {
        self.audible.get(track).copied().unwrap_or(true)
    }

    fn dispatch(&mut self, e: Event) {
        if matches!(e.action, Action::NoteOn { .. }) && !self.is_audible(e.track) {
            return;
        }
        let Some(voice) = self.program.voices.get(e.track) else {
            return;
        };
        let (main, fallback) = (voice.channel, voice.fallback);
        let keys = self.bank_of(voice.instrument).map(|b| b.keys.clone());
        // A note the sample bank did not record goes to the General MIDI channel.
        let channel_for = |pitch: u8| match &keys {
            Some(keys) if !keys.contains(&pitch) => fallback,
            _ => main,
        };
        let events = match e.action {
            Action::NoteOn { pitch, velocity } => [
                Some(MidiEvent::NoteOn {
                    channel: channel_for(pitch),
                    key: pitch,
                    vel: velocity,
                }),
                None,
            ],
            Action::NoteOff { pitch } => [
                Some(MidiEvent::NoteOff {
                    channel: channel_for(pitch),
                    key: pitch,
                }),
                None,
            ],
            // The wheel moves the notes of both channels together.
            Action::PitchBend(value) => [
                Some(MidiEvent::PitchBend {
                    channel: main,
                    value,
                }),
                keys.is_some().then_some(MidiEvent::PitchBend {
                    channel: fallback,
                    value,
                }),
            ],
        };
        // An out-of-range note must not interrupt playback.
        for event in events.into_iter().flatten() {
            let _ = self.synth.send_event(event);
        }
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

    #[test]
    fn seeking_past_a_note_leaves_only_what_comes_after() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = two_notes(None);
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&score);

        // The second half note starts at tick 64 (1 s at 120 bpm).
        player.seek(64.0);
        assert!((player.position_ticks() - 64.0).abs() < 0.5);
        assert!(!player.is_playing());
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100 / 2];
        player.render(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.01), "the second note plays");

        // Seeking into the middle of the second note: its attack is behind, so only silence.
        let mut other =
            Player::new(&mut File::open(test_soundfont().unwrap()).unwrap(), 44_100).unwrap();
        other.load_score(&score);
        other.seek(96.0);
        other.play();
        other.render(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            peak < 1e-3,
            "seeking should not restart the note (peak {peak})"
        );
    }

    #[test]
    fn seeking_is_kept_inside_the_score() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&two_notes(None));
        for wild in [-5.0, f64::NAN, f64::INFINITY, 1e12] {
            player.seek(wild);
            assert!(player.position() < player.program.end_sample, "{wild}");
        }
        player.seek(1e12);
        player.play();
        player.render(&mut vec![0.0f32; 2 * 44_100]);
        assert!(
            !player.is_playing(),
            "the end of the score stops the player"
        );
    }

    #[test]
    fn a_track_that_is_not_audible_stays_silent() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        player.load_score(&one_note_score());
        player.set_audible(vec![false]);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100];
        player.render(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1e-3, "muted track is audible (peak {peak})");

        // Audible again from the next note on; a reload keeps the choice.
        player.set_audible(vec![true]);
        player.load_score(&one_note_score());
        player.play();
        player.render(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.01));
    }

    /// A sample bank fetched by `pnpm fetch:soundfont`, or `None` to skip the test.
    fn test_bank(file: &str) -> Option<std::path::PathBuf> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/soundfonts/banks")
            .join(file);
        path.exists().then_some(path)
    }

    fn render_with_bank(
        base: &std::path::Path,
        bank: Option<(Instrument, &std::path::Path, RangeInclusive<u8>)>,
        score: &Score,
    ) -> Vec<f32> {
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        if let Some((instrument, path, keys)) = bank {
            player
                .add_bank(instrument, &mut File::open(path).unwrap(), keys)
                .unwrap();
        }
        player.load_score(score);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100];
        player.render(&mut buf);
        buf
    }

    #[test]
    fn a_sample_bank_changes_the_sound_of_its_instrument_and_nothing_else() {
        let (Some(base), Some(nylon)) = (test_soundfont(), test_bank("nylon-guitar.sf2")) else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let guitar = one_note_score();
        let mut bass = one_note_score();
        bass.tracks[0].instrument = Instrument::Bass;
        bass.tracks[0].tuning = Instrument::Bass.standard_tuning().to_vec();

        let plain = render_with_bank(&base, None, &guitar);
        let sampled = render_with_bank(&base, Some((Instrument::Guitar, &nylon, 0..=127)), &guitar);
        assert_ne!(plain, sampled, "the recorded guitar sounds different");
        let peak = sampled.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01 && peak <= 1.0, "peak {peak}");

        // A bank for the guitar leaves the bass on the base SoundFont.
        let bass_plain = render_with_bank(&base, None, &bass);
        let bass_with_guitar_bank =
            render_with_bank(&base, Some((Instrument::Guitar, &nylon, 0..=127)), &bass);
        assert_eq!(bass_plain, bass_with_guitar_bank);
    }

    #[test]
    fn every_bank_plays_its_instrument_over_the_whole_neck() {
        let Some(base) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        for bank in &crate::SAMPLE_BANKS {
            let Some(path) = test_bank(bank.file) else {
                eprintln!("{} not fetched, skipped", bank.file);
                continue;
            };
            let mut score = one_note_score();
            score.tracks[0].instrument = bank.instrument;
            score.tracks[0].tuning = bank.instrument.standard_tuning().to_vec();
            let strings = score.tracks[0].tuning.len() as u8;
            // Every string open and at the twelfth fret: the recorded range and beyond it.
            for string in 1..=strings {
                for fret in [0, 12, 24] {
                    let note = &mut score.tracks[0].measures[0].voices[0].beats[0].notes[0];
                    (note.string, note.fret) = (string, fret);
                    let buf = render_with_bank(
                        &base,
                        Some((bank.instrument, &path, bank.keys())),
                        &score,
                    );
                    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                    assert!(
                        peak > 0.005,
                        "{} string {string} fret {fret} is silent",
                        bank.file
                    );
                    assert!(peak <= 1.0, "{} clips", bank.file);
                }
            }
        }
    }

    #[test]
    fn a_note_the_bank_did_not_record_is_played_with_the_base_sound() {
        let (Some(base), Some(bass_bank)) = (test_soundfont(), test_bank("electric-bass.sf2"))
        else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let bank = crate::SAMPLE_BANKS
            .iter()
            .find(|b| b.instrument == Instrument::Bass)
            .unwrap();
        let mut score = one_note_score();
        score.tracks[0].instrument = Instrument::Bass;
        score.tracks[0].tuning = Instrument::Bass.standard_tuning().to_vec();
        let note = &mut score.tracks[0].measures[0].voices[0].beats[0].notes[0];
        // G2 string at the 24th fret is G4 (67), above the recorded keys.
        (note.string, note.fret) = (1, 24);
        assert!(67 > bank.last_key);

        let with_bank = render_with_bank(
            &base,
            Some((Instrument::Bass, &bass_bank, bank.keys())),
            &score,
        );
        let without = render_with_bank(&base, None, &score);
        let difference = with_bank
            .iter()
            .zip(&without)
            .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(
            difference < 1e-4,
            "the fallback should sound like the base SoundFont ({difference})"
        );
    }

    #[test]
    fn the_declared_key_ranges_match_the_recordings() {
        for bank in &crate::SAMPLE_BANKS {
            let Some(path) = test_bank(bank.file) else {
                eprintln!("{} not fetched, skipped", bank.file);
                continue;
            };
            let font = SoundFont::load(&mut File::open(path).unwrap()).unwrap();
            let mut synth = Synth::new(SynthDescriptor {
                sample_rate: 44_100.0,
                ..Default::default()
            })
            .unwrap();
            synth.add_font(font, true);

            let audible: Vec<u8> = (0..128u8)
                .filter(|&key| {
                    let _ = synth.send_event(MidiEvent::AllSoundOff { channel: 0 });
                    // Let the reverb of the key before die out, or it is heard as this key.
                    for _ in 0..44_100 {
                        synth.read_next();
                    }
                    let _ = synth.send_event(MidiEvent::NoteOn {
                        channel: 0,
                        key,
                        vel: 100,
                    });
                    let peak = (0..4410).fold(0.0f32, |m, _| {
                        let (l, r) = synth.read_next();
                        m.max(l.abs()).max(r.abs())
                    });
                    peak > 0.001
                })
                .collect();
            assert_eq!(
                (audible.first().copied(), audible.last().copied()),
                (Some(bank.first_key), Some(bank.last_key)),
                "{}",
                bank.file
            );
            assert_eq!(
                audible.len(),
                usize::from(bank.last_key - bank.first_key) + 1,
                "{} has gaps",
                bank.file
            );
        }
    }

    #[test]
    fn a_bank_that_is_not_a_soundfont_is_an_error_and_changes_nothing() {
        let Some(base) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        let mut junk = std::io::Cursor::new(vec![1u8; 100]);
        assert!(matches!(
            player.add_bank(Instrument::Bass, &mut junk, 0..=127),
            Err(PlayerError::SoundFont(_))
        ));
        assert!(player.bank_of(Instrument::Bass).is_none());
    }
}
