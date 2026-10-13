//! Score model: Score > Track > Measure > Voice > Beat > Note.

mod demo;
mod edit;
mod validate;

pub use demo::{blank_score, demo_score};
pub use edit::{apply_edit, Edit, EditError};
pub use validate::ScoreError;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    pub tempo_bpm: u16,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub instrument: Instrument,
    /// MIDI pitches of the open strings, from the highest (string 1) to the lowest.
    pub tuning: Vec<u8>,
    pub measures: Vec<Measure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Guitar,
    Bass,
    Violin,
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
    /// MIDI pitch of a note, or `None` if the string does not exist.
    pub fn pitch(&self, note: &Note) -> Option<u8> {
        let open = *self.tuning.get(note.string.checked_sub(1)? as usize)?;
        open.checked_add(note.fret).filter(|p| *p <= 127)
    }
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

impl Instrument {
    /// Open-string pitches, highest string first.
    pub fn standard_tuning(self) -> &'static [u8] {
        match self {
            Instrument::Guitar => &GUITAR_STANDARD,
            Instrument::Bass => &BASS_STANDARD,
            Instrument::Violin => &VIOLIN_STANDARD,
        }
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
            tuning: GUITAR_STANDARD.to_vec(),
            measures: vec![],
        }
    }

    #[test]
    fn standard_tunings_are_listed_highest_string_first() {
        for instrument in [Instrument::Guitar, Instrument::Bass, Instrument::Violin] {
            let tuning = instrument.standard_tuning();
            assert!(tuning.windows(2).all(|w| w[0] > w[1]), "{instrument:?}");
        }
        assert_eq!(Instrument::Bass.standard_tuning().len(), 4);
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
