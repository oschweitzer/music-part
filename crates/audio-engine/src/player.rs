use std::io::{Read, Seek};
use std::ops::RangeInclusive;

use oxisynth::{MidiEvent, SoundFont, SoundFontId, Synth, SynthDescriptor};
use score_model::{Score, Sound};

use crate::sequencer::{self, Action, Event, Timbre, BEND_CENTER, BEND_RANGE_SEMITONES};
use crate::sfz::{self, SfzInstrument, SfzSound};

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
/// Where the channels of the muted and harmonic timbres start, two by two (recording and
/// fallback). A MIDI channel number is a byte, so there is room for 53 of these pairs: a score
/// that needs more plays the rest with the open timbre.
const TIMBRE_BASE: usize = FALLBACK_BASE + MAX_TRACKS;
const SYNTH_GAIN: f32 = 0.6;
/// The synthesizer wants a multiple of 16, and a channel number must fit in a byte.
const SYNTH_CHANNELS: u8 = 240;

/// Two MIDI channels that play one timbre of a track.
#[derive(Clone, Copy)]
struct Lane {
    channel: u8,
    /// Plays the notes a sample bank has no recording for, with the General MIDI sound.
    fallback: u8,
}

/// The MIDI channels of a track, and what it plays.
struct Channel {
    open: Lane,
    /// The lanes of the muted, harmonic and legato timbres, when the score uses them.
    muted: Option<Lane>,
    harmonic: Option<Lane>,
    legato: Option<Lane>,
    /// General MIDI program of the sound, used when no sample bank is set for it.
    program: u8,
    sound: Sound,
}

impl Channel {
    /// The lane for a timbre; the open one when the score gave the timbre none.
    fn lane(&self, timbre: Timbre) -> Lane {
        match timbre {
            Timbre::Open => self.open,
            Timbre::Muted => self.muted.unwrap_or(self.open),
            Timbre::Harmonic => self.harmonic.unwrap_or(self.open),
            Timbre::Legato => self.legato.unwrap_or(self.open),
        }
    }

    /// Every lane with its timbre.
    fn lanes(&self) -> impl Iterator<Item = (Timbre, Lane)> {
        [
            Some((Timbre::Open, self.open)),
            self.muted.map(|lane| (Timbre::Muted, lane)),
            self.harmonic.map(|lane| (Timbre::Harmonic, lane)),
            self.legato.map(|lane| (Timbre::Legato, lane)),
        ]
        .into_iter()
        .flatten()
    }

    fn channels(&self) -> impl Iterator<Item = u8> {
        self.lanes()
            .flat_map(|(_, lane)| [lane.channel, lane.fallback])
    }
}

/// A SoundFont that replaces the General MIDI program of one sound.
struct Bank {
    sound: Sound,
    /// Bank number the SoundFont answers to.
    number: u8,
    /// Keys the recordings cover; notes outside are played with the General MIDI sound.
    keys: RangeInclusive<u8>,
    /// The SoundFont in the synthesizer, to change its bank number or remove it.
    font: SoundFontId,
}

/// A sample bank read from its file and ready to add to a [`Player`]. Reading one takes a while
/// (the samples are decoded and copied), so it is done away from the real-time audio thread and
/// the result handed over; see [`Player::add_bank_font`].
pub struct BankFont(SoundFont);

impl BankFont {
    /// Reads a SoundFont 2 file. A corrupted file is an error, not a panic.
    pub fn read<R: Read + Seek>(soundfont: &mut R) -> Result<Self, PlayerError> {
        read_soundfont(soundfont).map(Self)
    }
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
        // percussion channel 9, then two more for each timbre a track actually uses.
        let mut voices: Vec<Channel> = score
            .tracks
            .iter()
            .take(MAX_TRACKS)
            .enumerate()
            .map(|(i, track)| Channel {
                open: Lane {
                    channel: (i + usize::from(i >= 9)) as u8,
                    fallback: (FALLBACK_BASE + i) as u8,
                },
                muted: None,
                harmonic: None,
                legato: None,
                program: track.sound.midi_program(),
                sound: track.sound,
            })
            .collect();
        let mut next_lane = TIMBRE_BASE;
        let mut take_lane = || {
            let lane = (next_lane + 2 <= usize::from(SYNTH_CHANNELS)).then_some(Lane {
                channel: next_lane as u8,
                fallback: (next_lane + 1) as u8,
            });
            next_lane += 2;
            lane
        };
        // Events come sorted by time, so lanes are handed out in order of first use.
        for event in &events {
            let Action::NoteOn { timbre, .. } = event.action else {
                continue;
            };
            let Some(voice) = voices.get_mut(event.track) else {
                continue;
            };
            let slot = match timbre {
                Timbre::Open => continue,
                Timbre::Muted => &mut voice.muted,
                Timbre::Harmonic => &mut voice.harmonic,
                Timbre::Legato => &mut voice.legato,
            };
            if slot.is_none() {
                *slot = take_lane();
            }
        }

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
/// Controllers of a SoundFont NRPN message: the select number (120 for a generator, then the
/// generator) and the data entry that sets its value.
const NRPN_MSB: u8 = 99;
const NRPN_LSB: u8 = 98;
const DATA_ENTRY_MSB: u8 = 6;
const DATA_ENTRY_LSB: u8 = 38;
/// SoundFont generators a timbre offsets.
const GEN_FILTER_FC: u8 = 8;
const GEN_VOL_ENV_ATTACK: u8 = 34;
const GEN_VOL_ENV_RELEASE: u8 = 38;
/// One step of NRPN data is two cents (or timecents) for these generators.
const NRPN_STEP: i32 = 2;

/// How a timbre differs from the sound of the SoundFont, as (generator, offset in cents).
fn timbre_offsets(timbre: Timbre) -> &'static [(u8, i32)] {
    match timbre {
        Timbre::Open => &[],
        // About four octaves below the open filter (20 kHz to 1.2 kHz): dull. A release of a
        // twentieth of the usual length: a muted string does not ring on.
        Timbre::Muted => &[(GEN_FILTER_FC, -4800), (GEN_VOL_ENV_RELEASE, -5000)],
        // About two and a half octaves down: the few overtones of a harmonic.
        Timbre::Harmonic => &[(GEN_FILTER_FC, -3000)],
        // An attack of about ten milliseconds instead of a pick: the note swells in, and a
        // little duller since nothing struck the string.
        Timbre::Legato => &[(GEN_VOL_ENV_ATTACK, 4000), (GEN_FILTER_FC, -1200)],
    }
}
/// Sample banks a player can hold at once. Bank numbers stay well under the 128 a controller can
/// carry.
pub const MAX_BANKS: u8 = 32;
/// Bank number of the first sample bank added; see `Player::add_bank`.
const FIRST_BANK: u8 = 100;

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
    /// Sample banks: the bank number the SoundFont of each sound answers to.
    banks: Vec<Bank>,
    /// SFZ instruments and the voices that play them (see `docs/sfz.md`).
    sfz: Vec<SfzSound>,
    sfz_voices: sfz::Voices,
    /// Every bank, SoundFont or SFZ, in the order added: the last one of a sound plays it.
    order: Vec<(Sound, BankKind)>,
}

/// The two kinds of sample bank a sound can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankKind {
    Font,
    Sfz,
}

/// The samples of a bank taken out of a [`Player`], to be freed away from the audio thread.
pub enum RemovedBank {
    Font(BankFont),
    Sfz(SfzInstrument),
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
            banks: Vec::with_capacity(usize::from(MAX_BANKS)),
            // Room for every bank up front: adding one happens on the audio thread.
            sfz: Vec::with_capacity(usize::from(MAX_BANKS)),
            sfz_voices: sfz::Voices::new(),
            order: Vec::with_capacity(2 * usize::from(MAX_BANKS)),
        })
    }

    /// Plays the tracks with `sound` with another SoundFont (a sample bank made of one
    /// instrument, as its first preset), instead of the General MIDI program of the base one.
    /// `keys` are the MIDI keys the recordings cover: a note outside them is played with the
    /// General MIDI sound rather than left silent. Call before loading a score. A second bank for
    /// the same sound replaces the first.
    pub fn add_bank<R: Read + Seek>(
        &mut self,
        sound: Sound,
        soundfont: &mut R,
        keys: RangeInclusive<u8>,
    ) -> Result<(), PlayerError> {
        self.add_bank_font(sound, BankFont::read(soundfont)?, keys)
    }

    /// [`Player::add_bank`] with a file already read, so that the slow part can happen on another
    /// thread. Takes effect for the next score loaded. Banks can be added and removed in any order
    /// between two loads.
    pub fn add_bank_font(
        &mut self,
        sound: Sound,
        BankFont(font): BankFont,
        keys: RangeInclusive<u8>,
    ) -> Result<(), PlayerError> {
        // Bank 0 is the base SoundFont. Each bank is shifted to a number counting down from
        // `FIRST_BANK`, so the most recent one, on top of the synthesizer's stack of fonts, has the
        // lowest. The synthesizer subtracts a font's shift from the requested bank and clamps at
        // zero: with the lowest shift on top, a request for another bank asks a font for a bank it
        // does not have and moves on, instead of landing on its bank 0.
        let number = u8::try_from(self.banks.len())
            .ok()
            .filter(|n| *n < MAX_BANKS)
            .map(|n| FIRST_BANK - n)
            .ok_or_else(|| PlayerError::SoundFont("too many sample banks".into()))?;
        // `number` follows from the count of banks held, so the newest is always the lowest:
        // `remove_bank` renumbers the banks after the one it removes to keep it so.
        let id = self.synth.add_font(font, false);
        self.synth.set_bank_offset(id, u32::from(number));
        self.order.push((sound, BankKind::Font));
        self.banks.push(Bank {
            sound,
            number,
            keys,
            font: id,
        });
        Ok(())
    }

    /// Plays the tracks with `sound` with an SFZ instrument, instead of the General MIDI program.
    /// `keys` are the MIDI keys it covers, as for `add_bank`; a note inside them that no region
    /// answers is also played by the General MIDI program. Takes effect for the next score
    /// loaded. The latest bank added for a sound is heard, SoundFont or SFZ; removing it brings
    /// back the one before.
    pub fn add_sfz(
        &mut self,
        sound: Sound,
        instrument: SfzInstrument,
        keys: RangeInclusive<u8>,
    ) -> Result<(), PlayerError> {
        if self.sfz.len() >= usize::from(MAX_BANKS) {
            return Err(PlayerError::SoundFont("too many SFZ instruments".into()));
        }
        // Voices point into the list by index: none may sound while it changes.
        self.sfz_voices.cut_all();
        self.sfz.push(SfzSound {
            sound,
            keys,
            instrument,
        });
        self.order.push((sound, BankKind::Sfz));
        Ok(())
    }

    /// Removes the SFZ instrument that plays `sound` (the most recent one), and returns it for
    /// the caller to free away from the real-time audio thread.
    pub fn remove_sfz(&mut self, sound: Sound) -> Option<SfzInstrument> {
        let index = self.sfz.iter().rposition(|s| s.sound == sound)?;
        self.sfz_voices.cut_all();
        let removed = self.sfz.remove(index);
        self.forget(sound, BankKind::Sfz);
        Some(removed.instrument)
    }

    /// Removes the bank that plays `sound` now, SoundFont or SFZ, so the sound goes back to the
    /// one it had before. Returns its samples, for the caller to free away from the audio thread.
    pub fn remove_latest_bank(&mut self, sound: Sound) -> Option<RemovedBank> {
        match self.latest_kind(sound)? {
            BankKind::Font => self.remove_bank(sound).map(RemovedBank::Font),
            BankKind::Sfz => self.remove_sfz(sound).map(RemovedBank::Sfz),
        }
    }

    /// Which kind of bank plays `sound`: the one added last.
    fn latest_kind(&self, sound: Sound) -> Option<BankKind> {
        self.order
            .iter()
            .rev()
            .find(|(s, _)| *s == sound)
            .map(|(_, kind)| *kind)
    }

    /// Drops the latest entry of `sound` and `kind` from the order the banks were added in.
    fn forget(&mut self, sound: Sound, kind: BankKind) {
        if let Some(at) = self.order.iter().rposition(|entry| *entry == (sound, kind)) {
            self.order.remove(at);
        }
    }

    /// The SFZ instrument that plays `sound`, by index, when it is the latest bank of the sound.
    fn sfz_of(&self, sound: Sound) -> Option<usize> {
        (self.latest_kind(sound) == Some(BankKind::Sfz))
            .then(|| self.sfz.iter().rposition(|s| s.sound == sound))
            .flatten()
    }

    /// Removes the sample bank that plays `sound` (the most recent one, if it was given several),
    /// so the sound goes back to the one it had before: an older bank or the General MIDI program.
    /// Takes effect for the next score loaded. Returns the bank's samples, for the caller to free
    /// away from the real-time audio thread.
    pub fn remove_bank(&mut self, sound: Sound) -> Option<BankFont> {
        let index = self.banks.iter().rposition(|b| b.sound == sound)?;
        let bank = self.banks.remove(index);
        self.forget(sound, BankKind::Font);
        let font = self.synth.remove_font(bank.font, false);
        // The banks added after it move up one number, back to `FIRST_BANK` minus their place:
        // fonts higher on the synthesizer's stack keep lower numbers, which `add_bank_font`
        // relies on.
        for (place, later) in self.banks.iter_mut().enumerate().skip(index) {
            later.number = FIRST_BANK - place as u8;
            self.synth
                .set_bank_offset(later.font, u32::from(later.number));
        }
        font.map(BankFont)
    }

    /// The sounds that have a sample bank, SoundFont or SFZ, oldest bank first.
    pub fn bank_sounds(&self) -> impl Iterator<Item = Sound> + '_ {
        self.order.iter().map(|(sound, _)| *sound)
    }

    /// Name of the preset the open channel of `track` plays: which bank a track got.
    #[cfg(test)]
    pub(crate) fn preset_of_track(&self, track: usize) -> Option<String> {
        let channel = self.program.voices.get(track)?.open.channel;
        self.synth
            .channel_preset(channel)
            .map(|preset| preset.name().to_string())
    }

    /// The SoundFont bank that plays `sound`, unless a later SFZ instrument does.
    fn bank_of(&self, sound: Sound) -> Option<&Bank> {
        if self.latest_kind(sound) != Some(BankKind::Font) {
            return None;
        }
        self.banks.iter().rev().find(|b| b.sound == sound)
    }

    pub fn sample_rate(&self) -> u32 {
        self.program.sample_rate
    }

    /// Replaces the score being played. Stops and rewinds.
    pub fn load(&mut self, program: Program) {
        self.stop();
        self.sfz_voices.cut_all();
        for voice in &program.voices {
            // A sample bank for the sound sits at its own bank number, with the sound at
            // program 0; the fallback channel and sounds without a bank use the General MIDI
            // program of the base SoundFont.
            let bank = self.bank_of(voice.sound).map(|b| b.number);
            for (timbre, lane) in voice.lanes() {
                self.set_sound(
                    lane.channel,
                    bank.unwrap_or(0),
                    bank.map_or(voice.program, |_| 0),
                );
                self.set_sound(lane.fallback, 0, voice.program);
                self.set_timbre(lane.channel, timbre);
                self.set_timbre(lane.fallback, timbre);
            }
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

    /// Shapes the sound of a channel for a timbre with SoundFont NRPN messages, which offset a
    /// generator of every note the channel plays: a lower filter cutoff makes it duller, a shorter
    /// release lets a note stop short.
    fn set_timbre(&mut self, channel: u8, timbre: Timbre) {
        for (generator, offset) in timbre_offsets(timbre) {
            // The data is centred on 8192 and each step is `NRPN_STEP` units of the generator.
            let data = (8192 + offset / NRPN_STEP) as u16;
            for (ctrl, value) in [
                (NRPN_MSB, 120),
                (NRPN_LSB, *generator),
                (DATA_ENTRY_LSB, (data & 0x7f) as u8),
                (DATA_ENTRY_MSB, (data >> 7) as u8),
            ] {
                let _ = self.synth.send_event(MidiEvent::ControlChange {
                    channel,
                    ctrl,
                    value,
                });
            }
        }
        // Park the parameter selection so that no later data entry changes a generator.
        for ctrl in [101, 100] {
            let _ = self.synth.send_event(MidiEvent::ControlChange {
                channel,
                ctrl,
                value: 127,
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
            for channel in voice.channels() {
                let _ = self.synth.send_event(MidiEvent::AllSoundOff { channel });
            }
        }
        self.sfz_voices.cut_all();
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
                for channel in voice.channels() {
                    let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
                }
                self.sfz_voices.release_track(track);
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
        self.sfz_voices.release_all();
        for voice in &self.program.voices {
            for channel in voice.channels() {
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
            let (sfz_l, sfz_r) = self.sfz_voices.next_frame(&self.sfz);
            frame[0] = l + sfz_l;
            frame[1] = r + sfz_r;
            self.position += 1;
            if self.position >= self.program.end_sample {
                self.stop();
            }
        }
    }

    /// Plays an event with the SFZ instrument of `sound`, if it has one. True when nothing is
    /// left for the synthesizer to do: a note-on that a region answered. A note-off still goes
    /// to the synthesizer too, in case the note-on fell back to the General MIDI program.
    fn dispatch_sfz(&mut self, e: Event, sound: Sound) -> bool {
        let Some(index) = self.sfz_of(sound) else {
            return false;
        };
        match e.action {
            Action::NoteOn {
                pitch,
                velocity,
                timbre,
            } => {
                self.sfz[index].keys.contains(&pitch)
                    && self.sfz_voices.note_on(
                        &mut self.sfz,
                        index,
                        sfz::NoteOn {
                            track: e.track,
                            key: pitch,
                            velocity,
                            timbre,
                            sample_rate: self.program.sample_rate,
                        },
                    )
            }
            Action::NoteOff { pitch, .. } => {
                self.sfz_voices.note_off(e.track, pitch);
                false
            }
            Action::PitchBend(_) => false,
        }
    }

    fn is_audible(&self, track: usize) -> bool {
        self.audible.get(track).copied().unwrap_or(true)
    }

    fn dispatch(&mut self, e: Event) {
        if matches!(e.action, Action::NoteOn { .. }) && !self.is_audible(e.track) {
            return;
        }
        let Some(sound) = self.program.voices.get(e.track).map(|v| v.sound) else {
            return;
        };
        if self.dispatch_sfz(e, sound) {
            return;
        }
        let keys = self.bank_of(sound).map(|b| b.keys.clone());
        let Some(voice) = self.program.voices.get(e.track) else {
            return;
        };
        // A note the sample bank did not record goes to the General MIDI channel.
        let channel_for = |lane: Lane, pitch: u8| match &keys {
            Some(keys) if !keys.contains(&pitch) => lane.fallback,
            _ => lane.channel,
        };
        // An out-of-range note must not interrupt playback, so errors are ignored.
        match e.action {
            Action::NoteOn {
                pitch,
                velocity,
                timbre,
            } => {
                let _ = self.synth.send_event(MidiEvent::NoteOn {
                    channel: channel_for(voice.lane(timbre), pitch),
                    key: pitch,
                    vel: velocity,
                });
            }
            Action::NoteOff { pitch, timbre } => {
                let _ = self.synth.send_event(MidiEvent::NoteOff {
                    channel: channel_for(voice.lane(timbre), pitch),
                    key: pitch,
                });
            }
            // The wheel moves the notes of every channel of the track together.
            Action::PitchBend(value) => {
                self.sfz_voices.set_bend(e.track, value);
                for channel in voice.channels() {
                    let _ = self
                        .synth
                        .send_event(MidiEvent::PitchBend { channel, value });
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use score_model::*;
    use std::fs::File;

    pub(crate) fn one_note_score() -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            tracks: vec![Track {
                name: "g".into(),
                instrument: Instrument::Guitar,
                sound: Sound::GuitarNylon,
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
                            notes: vec![Note::fretted(1, 0)],
                        }],
                    }],
                }],
                clef: None,
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
    pub(crate) fn test_soundfont() -> Option<std::path::PathBuf> {
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
                technique,
                tied: false,
                ..Note::fretted(1, fret)
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

    /// Renders a score with the muted, harmonic and legato lanes taken away, so those notes use the open
    /// timbre: same notes, same velocities, only the timbre differs.
    fn render_open_only(path: &std::path::Path, score: &Score, seconds: usize) -> Vec<f32> {
        let mut player = Player::new(&mut File::open(path).unwrap(), 44_100).unwrap();
        let mut program = Program::new(score, 44_100);
        for voice in &mut program.voices {
            voice.muted = None;
            voice.harmonic = None;
            voice.legato = None;
        }
        player.load(program);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100 * seconds];
        player.render(&mut buf);
        buf
    }

    /// How much of the energy is in the high frequencies, left channel, between two instants in
    /// seconds: the energy of the sample-to-sample change over the energy of the signal.
    fn brightness(buf: &[f32], from: f64, to: f64) -> f64 {
        let left: Vec<f64> = buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| f64::from(frame[0]))
            .skip((from * 44_100.0) as usize)
            .take(((to - from) * 44_100.0) as usize)
            .collect();
        let change: f64 = left.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
        let energy: f64 = left.iter().map(|x| x * x).sum();
        change / energy.max(1e-12)
    }

    fn energy(buf: &[f32], from: f64, to: f64) -> f64 {
        buf.as_chunks::<2>()
            .0
            .iter()
            .skip((from * 44_100.0) as usize)
            .take(((to - from) * 44_100.0) as usize)
            .map(|frame| f64::from(frame[0]).powi(2))
            .sum()
    }

    #[test]
    fn a_palm_muted_note_is_duller_than_the_same_note_open() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = two_notes(Some(Technique::PalmMute));
        let muted = render_seconds(&path, &score, 1);
        let open = render_open_only(&path, &score, 1);

        let (muted, open) = (brightness(&muted, 0.0, 0.4), brightness(&open, 0.0, 0.4));
        assert!(muted < open * 0.5, "muted {muted}, open {open}");
    }

    #[test]
    fn a_palm_muted_note_dies_when_it_is_cut() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        // The half note lasts a second and the mute cuts it at half a second.
        let score = two_notes(Some(Technique::PalmMute));
        let muted = render_seconds(&path, &score, 1);
        let open = render_open_only(&path, &score, 1);

        let (muted, open) = (energy(&muted, 0.5, 0.62), energy(&open, 0.5, 0.62));
        assert!(muted < open * 0.25, "muted {muted}, open {open}");
    }

    #[test]
    fn a_hammer_on_swells_in_instead_of_starting_with_a_pick() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = two_notes(Some(Technique::HammerOn));
        let legato = render_seconds(&path, &score, 1);
        let open = render_open_only(&path, &score, 1);

        // The first five milliseconds: the same note and velocity, so only the onset differs.
        let (legato, open) = (energy(&legato, 0.0, 0.005), energy(&open, 0.0, 0.005));
        assert!(legato < open * 0.5, "legato {legato}, open {open}");
    }

    #[test]
    fn a_harmonic_is_duller_than_the_same_note_open() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = two_notes(Some(Technique::Harmonic));
        let harmonic = render_seconds(&path, &score, 1);
        let open = render_open_only(&path, &score, 1);

        let (harmonic, open) = (brightness(&harmonic, 0.0, 0.8), brightness(&open, 0.0, 0.8));
        assert!(harmonic < open * 0.7, "harmonic {harmonic}, open {open}");
    }

    #[test]
    fn a_timbre_does_not_leak_into_the_next_open_note() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        // The second note (fret 5) is open in both scores, and starts after the first one.
        let muted = render_seconds(&path, &two_notes(Some(Technique::PalmMute)), 2);
        let plain = render_seconds(&path, &two_notes(None), 2);

        let (muted, plain) = (brightness(&muted, 1.0, 1.4), brightness(&plain, 1.0, 1.4));
        assert!((muted / plain - 1.0).abs() < 0.2, "{muted} against {plain}");
    }

    #[test]
    fn a_score_using_every_timbre_on_every_track_keeps_its_channels_apart() {
        let mut score = one_note_score();
        let beat = |technique| Beat {
            duration: Duration {
                value: NoteValue::Half,
                dots: 0,
            },
            notes: vec![Note {
                technique,
                tied: false,
                ..Note::fretted(1, 5)
            }],
        };
        score.tracks[0].measures[0].voices[0].beats = vec![
            beat(Some(Technique::PalmMute)),
            beat(Some(Technique::Harmonic)),
        ];
        let track = score.tracks[0].clone();
        score.tracks = vec![track; MAX_TRACKS];

        let program = Program::new(&score, 44_100);
        let mut channels: Vec<u8> = program.voices.iter().flat_map(Channel::channels).collect();
        let used = channels.len();
        channels.sort_unstable();
        channels.dedup();

        assert_eq!(channels.len(), used, "a channel is shared by two lanes");
        assert!(channels.iter().all(|c| *c != 9 && *c < SYNTH_CHANNELS));
        // Past the last free pair a track keeps the open timbre instead of failing.
        assert!(program.voices.iter().any(|v| v.muted.is_none()));
        assert!(program.voices[0].muted.is_some());
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

    /// The sounds of the packs committed in `assets/soundfonts/packs`, with the path of their
    /// sample file. The files are fetched by `pnpm fetch:soundfont`; callers skip missing ones.
    fn committed_banks() -> Vec<(crate::pack::PackSound, std::path::PathBuf)> {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/soundfonts/packs");
        let (packs, errors) = crate::pack::discover(&[root]);
        assert!(errors.is_empty(), "{errors:?}");
        packs
            .iter()
            .flat_map(|pack| {
                pack.manifest
                    .sounds
                    .iter()
                    .map(|sound| (sound.clone(), pack.file(sound)))
            })
            .collect()
    }

    /// The fetched sample file of the committed pack that plays `sound`, or `None` to skip.
    fn test_bank(sound: Sound) -> Option<(crate::pack::PackSound, std::path::PathBuf)> {
        committed_banks()
            .into_iter()
            .find(|(bank, path)| bank.sound == sound && path.exists())
    }

    fn render_with_bank(
        base: &std::path::Path,
        bank: Option<(Sound, &std::path::Path, RangeInclusive<u8>)>,
        score: &Score,
    ) -> Vec<f32> {
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        if let Some((sound, path, keys)) = bank {
            player
                .add_bank(sound, &mut File::open(path).unwrap(), keys)
                .unwrap();
        }
        player.load_score(score);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100];
        player.render(&mut buf);
        buf
    }

    #[test]
    fn a_palm_muted_note_of_a_sample_bank_is_duller_than_the_bank_open() {
        let (Some(base), Some((_, nylon))) = (test_soundfont(), test_bank(Sound::GuitarNylon))
        else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let bank = || Some((Sound::GuitarNylon, nylon.as_path(), 29..=88));
        let muted = render_with_bank(&base, bank(), &two_notes(Some(Technique::PalmMute)));
        let general_midi = render_with_bank(&base, None, &two_notes(Some(Technique::PalmMute)));

        // The muted lane plays the recording, not the General MIDI stand-in.
        assert!(muted
            .iter()
            .zip(&general_midi)
            .any(|(a, b)| (a - b).abs() > 1e-3));
        let open = {
            let mut player = Player::new(&mut File::open(&base).unwrap(), 44_100).unwrap();
            player
                .add_bank(
                    Sound::GuitarNylon,
                    &mut File::open(&nylon).unwrap(),
                    29..=88,
                )
                .unwrap();
            let mut program = Program::new(&two_notes(Some(Technique::PalmMute)), 44_100);
            program.voices[0].muted = None;
            player.load(program);
            player.play();
            let mut buf = vec![0.0f32; 2 * 44_100];
            player.render(&mut buf);
            buf
        };
        let (muted, open) = (brightness(&muted, 0.0, 0.4), brightness(&open, 0.0, 0.4));
        assert!(muted < open * 0.5, "muted {muted}, open {open}");
    }

    #[test]
    fn each_instrument_plays_its_own_bank_when_all_banks_are_loaded() {
        let Some(base) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let banks = committed_banks();
        if banks.iter().any(|(_, path)| !path.exists()) {
            eprintln!("sample banks missing, test skipped");
            return;
        }
        for (bank, path) in &banks {
            let mut score = one_note_score();
            score.tracks[0].instrument = bank.sound.instrument();
            score.tracks[0].sound = bank.sound;
            // Bass notes: the fallback would hide a wrong bank for the high ones.
            score.tracks[0].tuning = vec![40, 35, 31, 28];
            let alone = render_with_bank(
                &base,
                Some((bank.sound, path.as_path(), bank.keys())),
                &score,
            );
            let mut player = Player::new(&mut File::open(&base).unwrap(), 44_100).unwrap();
            for (other, other_path) in &banks {
                player
                    .add_bank(
                        other.sound,
                        &mut File::open(other_path).unwrap(),
                        other.keys(),
                    )
                    .unwrap();
            }
            player.load_score(&score);
            player.play();
            let mut all = vec![0.0f32; 2 * 44_100];
            player.render(&mut all);

            assert!(
                alone == all,
                "{:?} does not play its own bank when the others are loaded",
                bank.sound
            );
        }
    }

    /// A one-track score playing a C major chord around middle C with `sound`, whose instrument
    /// is pitched (keyboards, synthesizer, voice).
    fn pitched_chord(sound: Sound) -> Score {
        let mut score = blank_score("chord");
        let edits = [
            Edit::SetInstrument {
                track: 0,
                instrument: sound.instrument(),
            },
            Edit::SetSound { track: 0, sound },
        ];
        let notes = [48, 60, 64, 67].map(|pitch| Edit::AddPitch {
            track: 0,
            measure: 0,
            beat: 0,
            pitch,
        });
        for edit in edits.iter().chain(&notes) {
            apply_edit(&mut score, edit).unwrap();
        }
        score
    }

    #[test]
    fn every_keyboard_synth_and_voice_sound_is_audible_and_distinct() {
        let Some(path) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let pitched = [
            Instrument::Piano,
            Instrument::Organ,
            Instrument::Synth,
            Instrument::Voice,
        ];
        let mut presets: Vec<(Sound, String)> = Vec::new();
        let mut renders: Vec<(Sound, Vec<f32>)> = Vec::new();
        for sound in Sound::ALL
            .into_iter()
            .filter(|s| pitched.contains(&s.instrument()))
        {
            let mut player = Player::new(&mut File::open(&path).unwrap(), 44_100).unwrap();
            player.load_score(&pitched_chord(sound));
            player.play();
            let mut buf = vec![0.0f32; 2 * 44_100];
            player.render(&mut buf);
            let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.01, "{sound:?} is nearly silent (peak {peak})");
            assert!(peak <= 1.0, "{sound:?} clips (peak {peak})");
            let preset = player.preset_of_track(0).expect("a preset");
            if let Some((other, _)) = presets.iter().find(|(_, p)| *p == preset) {
                panic!("{sound:?} and {other:?} both play {preset}");
            }
            presets.push((sound, preset));
            // The same chord played by two sounds differs, beyond the noise of rounding.
            for (other, earlier) in &renders {
                let difference: f32 = buf.iter().zip(earlier).map(|(a, b)| (a - b).abs()).sum();
                assert!(difference > 1.0, "{sound:?} sounds like {other:?}");
            }
            renders.push((sound, buf));
        }
        assert_eq!(presets.len(), 15);
    }

    #[test]
    fn the_sound_of_a_track_picks_its_general_midi_program() {
        let mut score = one_note_score();
        score.tracks[0].instrument = Instrument::ElectricGuitar;
        score.tracks[0].sound = Sound::ElectricGuitarDistortion;
        assert_eq!(Program::new(&score, 44_100).voices[0].program, 30);
        score.tracks[0].sound = Sound::ElectricGuitarClean;
        assert_eq!(Program::new(&score, 44_100).voices[0].program, 27);
    }

    #[test]
    fn a_sample_bank_plays_only_the_sound_it_recorded() {
        let (Some(base), Some((_, electric))) =
            (test_soundfont(), test_bank(Sound::ElectricGuitarClean))
        else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let mut score = one_note_score();
        score.tracks[0].instrument = Instrument::ElectricGuitar;
        score.tracks[0].sound = Sound::ElectricGuitarDistortion;
        let bank = Some((Sound::ElectricGuitarClean, electric.as_path(), 0..=127));
        // The clean recordings leave the distorted guitar on its General MIDI program.
        assert_eq!(
            render_with_bank(&base, bank.clone(), &score),
            render_with_bank(&base, None, &score)
        );
    }

    #[test]
    fn a_sample_bank_changes_the_sound_of_its_instrument_and_nothing_else() {
        let (Some(base), Some((_, nylon))) = (test_soundfont(), test_bank(Sound::GuitarNylon))
        else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let guitar = one_note_score();
        let mut bass = one_note_score();
        bass.tracks[0].instrument = Instrument::Bass;
        bass.tracks[0].sound = Sound::BassFinger;
        bass.tracks[0].tuning = Instrument::Bass.standard_tuning().to_vec();

        let plain = render_with_bank(&base, None, &guitar);
        let sampled = render_with_bank(&base, Some((Sound::GuitarNylon, &nylon, 0..=127)), &guitar);
        assert_ne!(plain, sampled, "the recorded guitar sounds different");
        let peak = sampled.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01 && peak <= 1.0, "peak {peak}");

        // A bank for the guitar leaves the bass on the base SoundFont.
        let bass_plain = render_with_bank(&base, None, &bass);
        let bass_with_guitar_bank =
            render_with_bank(&base, Some((Sound::GuitarNylon, &nylon, 0..=127)), &bass);
        assert_eq!(bass_plain, bass_with_guitar_bank);
    }

    #[test]
    fn every_bank_plays_its_instrument_over_the_whole_neck() {
        let Some(base) = test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        for (bank, path) in &committed_banks() {
            if !path.exists() {
                eprintln!("{} not fetched, skipped", bank.file);
                continue;
            }
            let mut score = one_note_score();
            score.tracks[0].instrument = bank.sound.instrument();
            score.tracks[0].sound = bank.sound;
            score.tracks[0].tuning = bank.sound.instrument().standard_tuning().to_vec();
            let strings = score.tracks[0].tuning.len() as u8;
            // Every string open and at the twelfth fret: the recorded range and beyond it.
            for string in 1..=strings {
                for fret in [0, 12, 24] {
                    let note = &mut score.tracks[0].measures[0].voices[0].beats[0].notes[0];
                    note.place = score_model::Place::Fretted { string, fret };
                    let buf =
                        render_with_bank(&base, Some((bank.sound, path, bank.keys())), &score);
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
        let (Some(base), Some((bank, bass_bank))) =
            (test_soundfont(), test_bank(Sound::BassFinger))
        else {
            eprintln!("no SoundFont or sample bank available, test skipped");
            return;
        };
        let mut score = one_note_score();
        score.tracks[0].instrument = Instrument::Bass;
        score.tracks[0].sound = Sound::BassFinger;
        score.tracks[0].tuning = Instrument::Bass.standard_tuning().to_vec();
        let note = &mut score.tracks[0].measures[0].voices[0].beats[0].notes[0];
        // G2 string at the 24th fret is G4 (67), above the recorded keys.
        note.place = score_model::Place::Fretted {
            string: 1,
            fret: 24,
        };
        assert!(67 > bank.last_key);

        let with_bank = render_with_bank(
            &base,
            Some((Sound::BassFinger, &bass_bank, bank.keys())),
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
        for (bank, path) in &committed_banks() {
            if !path.exists() {
                eprintln!("{} not fetched, skipped", bank.file);
                continue;
            }
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
            player.add_bank(Sound::BassFinger, &mut junk, 0..=127),
            Err(PlayerError::SoundFont(_))
        ));
        assert!(player.bank_of(Sound::BassFinger).is_none());
    }
}
