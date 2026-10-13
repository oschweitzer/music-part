//! Modèle de partition : Score > Track > Measure > Voice > Beat > Note.

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
    /// Hauteurs MIDI des cordes à vide, de la plus aiguë (corde 1) à la plus grave.
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
    /// Numéro de corde, 1 = la plus aiguë.
    pub string: u8,
    pub fret: u8,
    pub technique: Option<Technique>,
    /// Liaison : cette note prolonge la même hauteur du temps précédent de la
    /// voix (y compris à travers une barre de mesure) sans être rejouée.
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
    /// Durée en 128e de note (ronde = 128).
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

impl Duration {
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
    pub fn ticks(self) -> u32 {
        self.numerator as u32 * 128 / self.denominator as u32
    }
}

impl Track {
    /// Hauteur MIDI d'une note, ou `None` si la corde n'existe pas.
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
    /// Vrai si chaque voix remplit exactement la mesure.
    pub fn is_complete(&self) -> bool {
        let expected = self.time_signature.ticks();
        self.voices.iter().all(|v| v.ticks() == expected)
    }
}

/// Accordage standard de la guitare 6 cordes (E4 B3 G3 D3 A2 E2).
pub const GUITAR_STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

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
