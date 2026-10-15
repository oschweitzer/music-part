//! Score model: Score > Track > Measure > Voice > Beat > Note.

mod demo;
mod edit;
mod fretting;
mod sound;
mod validate;

pub use demo::{blank_score, demo_score};
pub use edit::{apply_edit, Edit, EditError};
pub use fretting::place_chord;
pub use sound::Sound;
pub use validate::ScoreError;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    pub tempo_bpm: u16,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "TrackFile")]
pub struct Track {
    pub name: String,
    pub instrument: Instrument,
    /// How the track sounds; always one of the sounds of `instrument`.
    pub sound: Sound,
    /// MIDI pitches of the open strings, from string 1 to the last. String 1 is the highest
    /// except on a banjo, whose short fifth string is tuned higher (see `Instrument::first_fret`).
    pub tuning: Vec<u8>,
    pub measures: Vec<Measure>,
}

/// A track as files store it. Files written before sounds existed have no `sound`: the track then
/// gets the default sound of its instrument, which is the sound it used to have.
#[derive(Deserialize)]
struct TrackFile {
    name: String,
    instrument: Instrument,
    #[serde(default)]
    sound: Option<Sound>,
    tuning: Vec<u8>,
    measures: Vec<Measure>,
}

impl From<TrackFile> for Track {
    fn from(file: TrackFile) -> Self {
        Track {
            sound: file
                .sound
                .unwrap_or_else(|| file.instrument.default_sound()),
            name: file.name,
            instrument: file.instrument,
            tuning: file.tuning,
            measures: file.measures,
        }
    }
}

/// An instrument family: it decides the strings, how frets are counted and the default tuning.
/// How it sounds is the track's `Sound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    /// Acoustic guitar, with nylon or steel strings (the name predates the steel sound).
    Guitar,
    ElectricGuitar,
    Bass,
    Violin,
    /// Five-string banjo, with its short fifth string.
    Banjo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub time_signature: TimeSignature,
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Voice {
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Beat {
    pub duration: Duration,
    /// Vide = silence.
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duration {
    pub value: NoteValue,
    pub dots: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// String number, 1 = the highest.
    pub string: u8,
    pub fret: u8,
    pub technique: Option<Technique>,
    /// Tie: this note continues the same pitch from the previous beat of the
    /// voice (across barlines too) without being replayed.
    #[serde(default)]
    pub tied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    HammerOn,
    PullOff,
    Slide,
    Bend,
    PalmMute,
    Harmonic,
}

impl NoteValue {
    /// Duration in 128th notes (whole note = 128).
    fn ticks(self) -> u32 {
        match self {
            NoteValue::Whole => 128,
            NoteValue::Half => 64,
            NoteValue::Quarter => 32,
            NoteValue::Eighth => 16,
            NoteValue::Sixteenth => 8,
            NoteValue::ThirtySecond => 4,
        }
    }
}

/// Highest fret a note can be placed on.
pub const MAX_FRET: u8 = 24;

impl Duration {
    /// At most two dots, and none on a 32nd note (its dotted length is not a multiple of a 32nd).
    pub fn is_valid(self) -> bool {
        self.dots <= 2 && (self.value != NoteValue::ThirtySecond || self.dots == 0)
    }

    pub fn ticks(self) -> u32 {
        let base = self.value.ticks();
        let mut total = base;
        let mut add = base;
        for _ in 0..self.dots {
            add /= 2;
            total += add;
        }
        total
    }
}

impl TimeSignature {
    /// A power-of-two denominator up to 32, and a measure that is a whole number of 32nd notes.
    pub fn is_valid(self) -> bool {
        let ticks = self.ticks();
        self.numerator != 0
            && self.denominator.is_power_of_two()
            && self.denominator <= 32
            && ticks >= 4
            && ticks.is_multiple_of(4)
    }

    /// Length of a measure in 128th notes. A denominator of 0 counts as an empty measure.
    pub fn ticks(self) -> u32 {
        (self.numerator as u32 * 128)
            .checked_div(self.denominator as u32)
            .unwrap_or(0)
    }
}

/// Splits a length into valid durations, taking the largest that fits each time. `None` if the length is not a
/// multiple of a 32nd note, which no combination of valid durations can add up to.
pub fn decompose_ticks(mut len: u32) -> Option<Vec<Duration>> {
    const VALUES: [NoteValue; 6] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ];
    let mut candidates: Vec<Duration> = VALUES
        .iter()
        .flat_map(|&value| (0..=2).map(move |dots| Duration { value, dots }))
        .filter(|d| d.is_valid())
        .collect();
    candidates.sort_by_key(|d| std::cmp::Reverse(d.ticks()));

    let mut out = Vec::new();
    while len > 0 {
        let d = *candidates.iter().find(|d| d.ticks() <= len)?;
        len -= d.ticks();
        out.push(d);
    }
    Some(out)
}

impl Track {
    /// MIDI pitch of a note, or `None` if the string or the fret does not exist.
    pub fn pitch(&self, note: &Note) -> Option<u8> {
        fret_pitch(self.instrument, &self.tuning, note.string, note.fret)
    }
}

/// MIDI pitch of a fret of a string (1 is the first) of an instrument, or `None` if the string or
/// the fret does not exist.
pub fn fret_pitch(instrument: Instrument, tuning: &[u8], string: u8, fret: u8) -> Option<u8> {
    let open = *tuning.get(usize::from(string.checked_sub(1)?))?;
    if fret == 0 {
        return Some(open);
    }
    let first = instrument.first_fret(string, tuning.len());
    let above_open = fret.checked_sub(first).filter(|s| *s > 0)?;
    open.checked_add(above_open).filter(|p| *p <= 127)
}

/// The fret that plays a MIDI pitch on a string of an instrument, or `None` if the string cannot
/// play it within `MAX_FRET`. The inverse of `fret_pitch`.
pub fn pitch_fret(instrument: Instrument, tuning: &[u8], string: u8, pitch: u8) -> Option<u8> {
    let open = *tuning.get(usize::from(string.checked_sub(1)?))?;
    let above_open = pitch.checked_sub(open)?;
    if above_open == 0 {
        return Some(0);
    }
    above_open
        .checked_add(instrument.first_fret(string, tuning.len()))
        .filter(|f| *f <= MAX_FRET)
}

impl Voice {
    pub fn ticks(&self) -> u32 {
        self.beats.iter().map(|b| b.duration.ticks()).sum()
    }
}

impl Measure {
    /// True if every voice fills the measure exactly.
    pub fn is_complete(&self) -> bool {
        let expected = self.time_signature.ticks();
        self.voices.iter().all(|v| v.ticks() == expected)
    }
}

/// Standard tuning of a 6-string guitar (E4 B3 G3 D3 A2 E2).
pub const GUITAR_STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

/// Standard tuning of a 4-string bass (G2 D2 A1 E1).
pub const BASS_STANDARD: [u8; 4] = [43, 38, 33, 28];

/// Standard tuning of a violin (E5 A4 D4 G3).
pub const VIOLIN_STANDARD: [u8; 4] = [76, 69, 62, 55];

/// Open G tuning of a five-string banjo, string 1 first: D4 B3 G3 D3, then the short fifth
/// string, G4.
pub const BANJO_STANDARD: [u8; 5] = [62, 59, 55, 50, 67];

/// The fret where the short fifth string of a banjo starts.
const BANJO_FIFTH_STRING_START: u8 = 5;

impl Instrument {
    pub const ALL: [Instrument; 5] = [
        Instrument::Guitar,
        Instrument::ElectricGuitar,
        Instrument::Bass,
        Instrument::Violin,
        Instrument::Banjo,
    ];

    /// Open-string pitches, string 1 first.
    pub fn standard_tuning(self) -> &'static [u8] {
        match self {
            Instrument::Guitar | Instrument::ElectricGuitar => &GUITAR_STANDARD,
            Instrument::Bass => &BASS_STANDARD,
            Instrument::Violin => &VIOLIN_STANDARD,
            Instrument::Banjo => &BANJO_STANDARD,
        }
    }

    /// The fret where a string starts, 0 for a string that runs the whole neck. The fifth string
    /// of a five-string banjo starts at fret 5: tablature numbers its frets like the other
    /// strings, so it has no frets 1 to 5 and its fret 6 is one semitone above the open string.
    pub fn first_fret(self, string: u8, string_count: usize) -> u8 {
        match self {
            Instrument::Banjo if string == 5 && string_count == 5 => BANJO_FIFTH_STRING_START,
            _ => 0,
        }
    }

    /// The instrument for a General MIDI program, when a file names one.
    pub fn from_midi_program(program: Option<u8>) -> Instrument {
        Sound::from_midi_program(program).instrument()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarter() -> Duration {
        Duration {
            value: NoteValue::Quarter,
            dots: 0,
        }
    }

    fn guitar() -> Track {
        Track {
            name: "Guitar".into(),
            instrument: Instrument::Guitar,
            sound: Sound::GuitarNylon,
            tuning: GUITAR_STANDARD.to_vec(),
            measures: vec![],
        }
    }

    #[test]
    fn standard_tunings_are_listed_highest_full_length_string_first() {
        for instrument in Instrument::ALL {
            let tuning = instrument.standard_tuning();
            let full_length: Vec<u8> = (1..=tuning.len() as u8)
                .filter(|&s| instrument.first_fret(s, tuning.len()) == 0)
                .map(|s| tuning[usize::from(s) - 1])
                .collect();
            assert!(
                full_length.windows(2).all(|w| w[0] > w[1]),
                "{instrument:?}"
            );
        }
        assert_eq!(Instrument::Bass.standard_tuning().len(), 4);
    }

    #[test]
    fn the_program_of_a_file_picks_the_instrument_of_its_sound() {
        assert_eq!(Instrument::from_midi_program(None), Instrument::Guitar);
        assert_eq!(Instrument::from_midi_program(Some(25)), Instrument::Guitar);
        assert_eq!(
            Instrument::from_midi_program(Some(30)),
            Instrument::ElectricGuitar
        );
        assert_eq!(Instrument::from_midi_program(Some(105)), Instrument::Banjo);
    }

    fn banjo() -> Track {
        Track {
            name: "Banjo".into(),
            instrument: Instrument::Banjo,
            sound: Sound::Banjo,
            tuning: BANJO_STANDARD.to_vec(),
            measures: vec![],
        }
    }

    fn at(string: u8, fret: u8) -> Note {
        Note {
            string,
            fret,
            technique: None,
            tied: false,
        }
    }

    #[test]
    fn the_short_fifth_banjo_string_counts_frets_from_the_neck() {
        let t = banjo();
        assert_eq!(t.pitch(&at(5, 0)), Some(67));
        for fret in 1..=5 {
            assert_eq!(t.pitch(&at(5, fret)), None, "fret {fret}");
        }
        assert_eq!(t.pitch(&at(5, 6)), Some(68));
        assert_eq!(t.pitch(&at(5, 7)), Some(69));
        // The long strings are fretted as usual.
        assert_eq!(t.pitch(&at(4, 2)), Some(52));
    }

    #[test]
    fn finding_the_fret_of_a_pitch_is_the_inverse_of_its_pitch() {
        for string in 1..=5 {
            for pitch in 40..=100 {
                if let Some(fret) = pitch_fret(Instrument::Banjo, &BANJO_STANDARD, string, pitch) {
                    assert_eq!(
                        fret_pitch(Instrument::Banjo, &BANJO_STANDARD, string, fret),
                        Some(pitch)
                    );
                }
            }
        }
        assert_eq!(
            pitch_fret(Instrument::Banjo, &BANJO_STANDARD, 5, 69),
            Some(7)
        );
        assert_eq!(
            pitch_fret(Instrument::Guitar, &GUITAR_STANDARD, 1, 69),
            Some(5)
        );
    }

    #[test]
    fn a_track_without_a_sound_gets_the_default_sound_of_its_instrument() {
        let json = r#"{"name":"B","instrument":"Bass","tuning":[43,38,33,28],"measures":[]}"#;
        let track: Track = serde_json::from_str(json).unwrap();
        assert_eq!(track.sound, Sound::BassFinger);
    }

    #[test]
    fn the_sound_of_a_track_is_saved_and_read_back() {
        let mut track = guitar();
        track.instrument = Instrument::ElectricGuitar;
        track.sound = Sound::ElectricGuitarDistortion;
        let json = serde_json::to_string(&track).unwrap();
        assert!(
            json.contains(r#""sound":"electric-guitar/distortion""#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Track>(&json).unwrap(), track);
    }

    #[test]
    fn decompose_uses_the_largest_durations_first() {
        let d = |value, dots| Duration { value, dots };
        assert_eq!(decompose_ticks(128), Some(vec![d(NoteValue::Whole, 0)]));
        assert_eq!(decompose_ticks(48), Some(vec![d(NoteValue::Quarter, 1)]));
        assert_eq!(
            decompose_ticks(36),
            Some(vec![
                d(NoteValue::Quarter, 0),
                d(NoteValue::ThirtySecond, 0)
            ])
        );
        assert_eq!(decompose_ticks(0), Some(vec![]));
    }

    #[test]
    fn decompose_always_adds_back_up_and_refuses_what_is_not_a_32nd_multiple() {
        for len in (0..=512).step_by(4) {
            let parts = decompose_ticks(len).unwrap();
            assert_eq!(parts.iter().map(|d| d.ticks()).sum::<u32>(), len);
            assert!(parts.iter().all(|d| d.is_valid()));
        }
        assert_eq!(decompose_ticks(6), None);
        assert_eq!(decompose_ticks(3), None);
    }

    #[test]
    fn a_dotted_32nd_is_not_a_valid_duration() {
        assert!(!Duration {
            value: NoteValue::ThirtySecond,
            dots: 1
        }
        .is_valid());
        assert!(!Duration {
            value: NoteValue::Quarter,
            dots: 3
        }
        .is_valid());
        assert!(Duration {
            value: NoteValue::Sixteenth,
            dots: 2
        }
        .is_valid());
    }

    #[test]
    fn a_zero_denominator_does_not_divide_by_zero() {
        assert_eq!(
            TimeSignature {
                numerator: 4,
                denominator: 0
            }
            .ticks(),
            0
        );
    }

    #[test]
    fn dotted_duration() {
        let d = Duration {
            value: NoteValue::Quarter,
            dots: 1,
        };
        assert_eq!(d.ticks(), 48);
    }

    #[test]
    fn pitch_from_string_and_fret() {
        let t = guitar();
        let open_e = Note {
            string: 6,
            fret: 0,
            technique: None,
            tied: false,
        };
        let a_string_5th = Note {
            string: 5,
            fret: 5,
            technique: None,
            tied: false,
        };
        assert_eq!(t.pitch(&open_e), Some(40));
        assert_eq!(t.pitch(&a_string_5th), Some(50));
        assert_eq!(
            t.pitch(&Note {
                string: 7,
                fret: 0,
                technique: None,
                tied: false,
            }),
            None
        );
        assert_eq!(
            t.pitch(&Note {
                string: 0,
                fret: 0,
                technique: None,
                tied: false,
            }),
            None
        );
    }

    #[test]
    fn measure_completeness() {
        let beat = Beat {
            duration: quarter(),
            notes: vec![],
        };
        let mut m = Measure {
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            voices: vec![Voice {
                beats: vec![beat.clone(); 4],
            }],
        };
        assert!(m.is_complete());
        m.voices[0].beats.pop();
        assert!(!m.is_complete());
    }

    #[test]
    fn note_without_tied_field_still_deserializes() {
        let note: Note = serde_json::from_str(r#"{"string":1,"fret":3,"technique":null}"#).unwrap();
        assert!(!note.tied);
    }

    #[test]
    fn json_roundtrip() {
        let score = Score {
            title: "Test".into(),
            tempo_bpm: 120,
            tracks: vec![guitar()],
        };
        let json = serde_json::to_string(&score).unwrap();
        let back: Score = serde_json::from_str(&json).unwrap();
        assert_eq!(score, back);
    }
}
