//! The voices that play SFZ regions, mixed with the synthesizer in `Player::render`.
//!
//! Real time: the pool is allocated once in `Voices::new`. Starting, releasing and rendering a
//! voice only reads the decoded samples and writes into the pool: no allocation, no lock, no
//! system call. See `docs/sfz.md`.

use crate::sequencer::{Timbre, BEND_CENTER, BEND_RANGE_SEMITONES};

use super::parse::LoopMode;
use super::SfzSound;

/// Voices that can sound at once, all SFZ sounds together. When they are all busy, the oldest
/// one is taken over.
pub const MAX_VOICES: usize = 64;
/// A region at `volume=0` and full velocity plays its sample at this level, about the level of
/// the General MIDI programs of the synthesizer.
pub const SFZ_GAIN: f32 = 0.5;
/// Tracks the pitch wheel is kept for; as many as a score can have.
const MAX_TRACKS: usize = 64;
/// Below this level a releasing voice is silent and freed (-80 dB).
const SILENT: f32 = 1e-4;
/// Shortest release, in seconds, so that a note-off never clicks.
const MIN_RELEASE: f32 = 0.005;
/// An exponential stage falls by 60 dB over its time.
const LN_MINUS_60_DB: f32 = -6.907_755;

/// The pool of voices, and the pitch wheel of each track.
pub struct Voices {
    voices: Vec<Voice>,
    /// Pitch ratio of the wheel of each track, 1 when centred.
    bend: [f64; MAX_TRACKS],
    /// State of the random generator for `lorand` and `hirand`.
    random: u32,
    /// Counts the notes started, to find the oldest voice.
    clock: u64,
}

impl Default for Voices {
    fn default() -> Self {
        Self::new()
    }
}

/// What a note-on asks of a sound.
#[derive(Debug, Clone, Copy)]
pub struct NoteOn {
    pub track: usize,
    pub key: u8,
    pub velocity: u8,
    pub timbre: Timbre,
    /// Output sample rate.
    pub sample_rate: u32,
}

impl Voices {
    pub fn new() -> Self {
        Self {
            voices: vec![Voice::default(); MAX_VOICES],
            bend: [1.0; MAX_TRACKS],
            random: 0x9E37_79B9,
            clock: 0,
        }
    }

    /// Starts a voice for each region of `sounds[sound]` that answers the note. False when none
    /// does: the note is then the General MIDI program's to play.
    pub fn note_on(&mut self, sounds: &mut [SfzSound], sound: usize, note: NoteOn) -> bool {
        let Some(sfz) = sounds.get_mut(sound) else {
            return false;
        };
        let random = self.next_random();
        let mut started = false;
        let instrument = &mut sfz.instrument;
        for index in 0..instrument.regions.len() {
            if !instrument.answers(index, note.key, note.velocity, random) {
                continue;
            }
            let region = &instrument.regions[index];
            let Some(sample) = instrument.samples.get(region.sample) else {
                continue;
            };
            let voice = Voice::start(VoiceStart {
                sound,
                region,
                sample_rate: sample.sample_rate,
                note,
                started: self.clock,
            });
            self.clock += 1;
            let slot = self.free_slot();
            self.voices[slot] = voice;
            started = true;
        }
        started
    }

    /// Releases the voices of a key of a track.
    pub fn note_off(&mut self, track: usize, key: u8) {
        for voice in &mut self.voices {
            if voice.is_active && voice.track == track && voice.key == key {
                voice.release();
            }
        }
    }

    /// Moves the pitch of every voice of a track, as the pitch wheel of its MIDI channels does.
    pub fn set_bend(&mut self, track: usize, value: u16) {
        if let Some(bend) = self.bend.get_mut(track) {
            let semitones = (f64::from(value) - f64::from(BEND_CENTER)) / f64::from(BEND_CENTER)
                * f64::from(BEND_RANGE_SEMITONES);
            *bend = (semitones / 12.0).exp2();
        }
    }

    /// Releases every voice of a track, letting them ring out as a note-off does.
    pub fn release_track(&mut self, track: usize) {
        for voice in &mut self.voices {
            if voice.track == track {
                voice.release();
            }
        }
    }

    /// Releases every voice and centres every pitch wheel.
    pub fn release_all(&mut self) {
        for voice in &mut self.voices {
            voice.release();
        }
        self.bend = [1.0; MAX_TRACKS];
    }

    /// Cuts every voice at once.
    pub fn cut_all(&mut self) {
        for voice in &mut self.voices {
            voice.is_active = false;
        }
        self.bend = [1.0; MAX_TRACKS];
    }

    /// Voices sounding, released ones included.
    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active).count()
    }

    /// The next stereo frame of every voice, mixed.
    pub fn next_frame(&mut self, sounds: &[SfzSound]) -> (f32, f32) {
        let (mut left, mut right) = (0.0, 0.0);
        for voice in &mut self.voices {
            if !voice.is_active {
                continue;
            }
            let bend = self.bend.get(voice.track).copied().unwrap_or(1.0);
            let sample = sounds
                .get(voice.sound)
                .and_then(|s| s.instrument.samples.get(voice.sample));
            let Some(sample) = sample else {
                voice.is_active = false;
                continue;
            };
            let (l, r) = voice.next(&sample.data, sample.channels, bend);
            left += l;
            right += r;
        }
        (left, right)
    }

    /// The slot for a new voice: a free one, else the oldest.
    fn free_slot(&self) -> usize {
        if let Some(free) = self.voices.iter().position(|v| !v.is_active) {
            return free;
        }
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.started)
            .map_or(0, |(index, _)| index)
    }

    /// A number in [0, 1) (xorshift32).
    fn next_random(&mut self) -> f32 {
        let mut x = self.random;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.random = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    is_active: bool,
    is_released: bool,
    /// Plays the whole sample, ignoring the note-off.
    is_one_shot: bool,
    sound: usize,
    sample: usize,
    track: usize,
    key: u8,
    started: u64,
    /// Position in the sample, in frames.
    position: f64,
    /// Frames of the sample per output frame, before the pitch wheel.
    step: f64,
    /// Loop, first frame and the frame after the last one, while it applies.
    loop_range: Option<(f64, f64)>,
    /// The loop stops at the note-off (`loop_sustain`).
    loops_until_release: bool,
    /// Past this frame the voice ends.
    last_frame: f64,
    gain_left: f32,
    gain_right: f32,
    stage: Stage,
    /// Output frames left in the delay or hold stage.
    frames_left: u32,
    level: f32,
    attack_step: f32,
    hold_frames: u32,
    decay_factor: f32,
    sustain: f32,
    release_factor: f32,
    /// One-pole low-pass coefficient, 1 for none.
    lowpass: f32,
    filtered: [f32; 2],
}

/// What starting a voice needs.
struct VoiceStart<'a> {
    sound: usize,
    region: &'a super::PlayRegion,
    sample_rate: u32,
    note: NoteOn,
    started: u64,
}

impl Voice {
    fn start(start: VoiceStart) -> Self {
        let VoiceStart {
            sound,
            region,
            sample_rate,
            note,
            started,
        } = start;
        let spec = &region.spec;
        let rate = note.sample_rate.max(1) as f32;
        let semitones =
            (f32::from(note.key) - f32::from(spec.pitch_keycenter)) * spec.pitch_keytrack / 100.0
                + spec.transpose as f32
                + spec.tune / 100.0;
        let step = f64::from((semitones / 12.0).exp2()) * f64::from(sample_rate)
            / f64::from(note.sample_rate.max(1));

        let velocity = f32::from(note.velocity.min(127)) / 127.0;
        let track = spec.amp_veltrack / 100.0;
        let curve = velocity * velocity;
        let velocity_gain = if track >= 0.0 {
            1.0 - track * (1.0 - curve)
        } else {
            1.0 + track * curve
        };
        let gain = SFZ_GAIN * 10f32.powf(spec.volume / 20.0) * spec.amplitude / 100.0
            * velocity_gain.max(0.0);
        let pan = spec.pan / 100.0;
        // Constant power for a mono sample, a balance for a stereo one.
        let (pan_left, pan_right) = if region.channels == 1 {
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            (
                angle.cos() * std::f32::consts::SQRT_2,
                angle.sin() * std::f32::consts::SQRT_2,
            )
        } else {
            ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0))
        };

        let (cutoff, release_scale, min_attack) = timbre_shape(note.timbre);
        let env = spec.envelope;
        let frames = |seconds: f32| (seconds.max(0.0) * rate) as u32;
        let falls_in = |seconds: f32| {
            if seconds <= 0.0 {
                0.0
            } else {
                (LN_MINUS_60_DB / (seconds * rate)).exp()
            }
        };
        let attack = env.attack.max(min_attack);
        let release = (env.release * release_scale).max(MIN_RELEASE);
        let lowpass = match cutoff {
            Some(hz) => 1.0 - (-std::f32::consts::TAU * hz / rate).exp(),
            None => 1.0,
        };

        let (is_one_shot, loop_range, loops_until_release) = match region.loop_mode {
            LoopMode::OneShot => (true, None, false),
            LoopMode::NoLoop => (false, None, false),
            LoopMode::Continuous => (false, region.loop_range(), false),
            LoopMode::Sustain => (false, region.loop_range(), true),
        };
        Self {
            is_active: true,
            is_released: false,
            is_one_shot,
            sound,
            sample: region.sample,
            track: note.track,
            key: note.key,
            started,
            position: region.first_frame as f64,
            step,
            loop_range,
            loops_until_release,
            last_frame: region.last_frame as f64,
            gain_left: gain * pan_left,
            gain_right: gain * pan_right,
            stage: Stage::Delay,
            frames_left: frames(env.delay),
            level: 0.0,
            attack_step: if attack <= 0.0 {
                1.0
            } else {
                1.0 / (attack * rate).max(1.0)
            },
            hold_frames: frames(env.hold),
            decay_factor: falls_in(env.decay),
            sustain: env.sustain / 100.0,
            release_factor: falls_in(release),
            lowpass,
            filtered: [0.0; 2],
        }
    }

    fn release(&mut self) {
        if !self.is_active || self.is_released || self.is_one_shot {
            return;
        }
        self.is_released = true;
        self.stage = Stage::Release;
        if self.loops_until_release {
            self.loop_range = None;
        }
    }

    /// The next output frame of this voice, and moves it on.
    fn next(&mut self, data: &[f32], channels: usize, bend: f64) -> (f32, f32) {
        let level = self.envelope();
        if !self.is_active {
            return (0.0, 0.0);
        }
        let (mut left, mut right) = self.read(data, channels);
        if self.lowpass < 1.0 {
            self.filtered[0] += self.lowpass * (left - self.filtered[0]);
            self.filtered[1] += self.lowpass * (right - self.filtered[1]);
            (left, right) = (self.filtered[0], self.filtered[1]);
        }

        self.position += self.step * bend;
        match self.loop_range {
            Some((first, end)) => {
                if self.position >= end {
                    self.position = first + (self.position - first) % (end - first);
                }
            }
            None => {
                if self.position > self.last_frame {
                    self.is_active = false;
                }
            }
        }
        (
            left * level * self.gain_left,
            right * level * self.gain_right,
        )
    }

    /// The sample at the current position, interpolated between two frames.
    fn read(&self, data: &[f32], channels: usize) -> (f32, f32) {
        let channels = channels.max(1);
        let frame = self.position as usize;
        let fraction = (self.position - frame as f64) as f32;
        let frames = data.len() / channels;
        if frame >= frames {
            return (0.0, 0.0);
        }
        let next = match self.loop_range {
            Some((first, end)) if frame + 1 >= end as usize => first as usize,
            _ => (frame + 1).min(frames - 1),
        };
        let value = |frame: usize, channel: usize| {
            data.get(frame * channels + channel.min(channels - 1))
                .copied()
                .unwrap_or(0.0)
        };
        let at = |channel| {
            let a = value(frame, channel);
            a + (value(next, channel) - a) * fraction
        };
        (at(0), at(1))
    }

    /// The envelope level for this frame, and moves it on. Ends the voice once released and
    /// silent.
    fn envelope(&mut self) -> f32 {
        match self.stage {
            Stage::Delay => {
                if self.frames_left == 0 {
                    self.stage = Stage::Attack;
                    return self.envelope();
                }
                self.frames_left -= 1;
            }
            Stage::Attack => {
                self.level = (self.level + self.attack_step).min(1.0);
                if self.level >= 1.0 {
                    self.stage = Stage::Hold;
                    self.frames_left = self.hold_frames;
                }
            }
            Stage::Hold => {
                if self.frames_left == 0 {
                    self.stage = Stage::Decay;
                } else {
                    self.frames_left -= 1;
                }
            }
            Stage::Decay => {
                self.level = self.sustain + (self.level - self.sustain) * self.decay_factor;
                if self.level - self.sustain < SILENT {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                // A plucked sound often decays to nothing: free its voice.
                if self.level < SILENT {
                    self.is_active = false;
                }
            }
            Stage::Release => {
                self.level *= self.release_factor;
                if self.level < SILENT {
                    self.is_active = false;
                }
            }
        }
        self.level
    }
}

/// How a timbre changes an SFZ voice, as `Player::set_timbre` changes a SoundFont channel: low-
/// pass cutoff in hertz, factor on the release time, shortest attack in seconds.
fn timbre_shape(timbre: Timbre) -> (Option<f32>, f32, f32) {
    match timbre {
        Timbre::Open => (None, 1.0, 0.0),
        Timbre::Muted => (Some(1_200.0), 1.0 / 20.0, 0.0),
        Timbre::Harmonic => (Some(3_500.0), 1.0, 0.0),
        Timbre::Legato => (Some(10_000.0), 1.0, 0.010),
    }
}
