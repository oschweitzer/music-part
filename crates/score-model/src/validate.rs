//! Checks that a score is safe to edit, display and play. Scores built by this program always
//! pass; the check is for files written by hand or by another version.

use std::fmt;

use crate::{Score, MAX_FRET};

const MAX_TRACKS: usize = 64;
const MAX_STRINGS: usize = 12;
const MAX_MEASURES: usize = 5000;
const MAX_TEMPO: u16 = 999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreError(String);

impl fmt::Display for ScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScoreError {}

fn fail<T>(message: String) -> Result<T, ScoreError> {
    Err(ScoreError(message))
}

impl Score {
    /// Verifies the invariants the rest of the program relies on: sizes within limits, valid time
    /// signatures and durations, a sound of the track's instrument, notes on existing strings and
    /// frets, and every voice filling its measure.
    pub fn validate(&self) -> Result<(), ScoreError> {
        if !(1..=MAX_TEMPO).contains(&self.tempo_bpm) {
            return fail(format!("the tempo must be between 1 and {MAX_TEMPO} bpm"));
        }
        if self.tracks.len() > MAX_TRACKS {
            return fail(format!("a score cannot have more than {MAX_TRACKS} tracks"));
        }

        for track in &self.tracks {
            let name = &track.name;
            if track.sound.instrument() != track.instrument {
                return fail(format!(
                    "track \"{name}\" has the sound \"{}\", which is not a sound of its instrument",
                    track.sound.id()
                ));
            }
            if !(1..=MAX_STRINGS).contains(&track.tuning.len()) {
                return fail(format!(
                    "track \"{name}\" must have between 1 and {MAX_STRINGS} strings"
                ));
            }
            if track.tuning.iter().any(|&pitch| pitch > 127) {
                return fail(format!(
                    "track \"{name}\" has a string tuned above MIDI pitch 127"
                ));
            }
            if track.measures.len() > MAX_MEASURES {
                return fail(format!(
                    "track \"{name}\" has more than {MAX_MEASURES} measures"
                ));
            }

            for (m, measure) in track.measures.iter().enumerate() {
                let at = format!("track \"{name}\", measure {}", m + 1);
                let signature = measure.time_signature;
                let ticks = signature.ticks();
                if !signature.is_valid() {
                    return fail(format!(
                        "{at}: unsupported time signature {}/{}",
                        signature.numerator, signature.denominator
                    ));
                }
                if measure.voices.is_empty() {
                    return fail(format!("{at}: has no voice"));
                }

                for voice in &measure.voices {
                    if voice.beats.iter().any(|b| !b.duration.is_valid()) {
                        return fail(format!("{at}: contains an unsupported note duration"));
                    }
                    if voice.ticks() != ticks {
                        return fail(format!("{at}: the notes do not fill the measure exactly"));
                    }
                    for note in voice.beats.iter().flat_map(|b| &b.notes) {
                        if note.string == 0 || usize::from(note.string) > track.tuning.len() {
                            return fail(format!(
                                "{at}: a note is on string {}, which does not exist",
                                note.string
                            ));
                        }
                        if note.fret > MAX_FRET || track.pitch(note).is_none() {
                            return fail(format!(
                                "{at}: a note is on fret {}, which is out of range",
                                note.fret
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{demo_score, Duration, NoteValue, Sound, TimeSignature};

    fn error(score: &Score) -> String {
        score.validate().unwrap_err().to_string()
    }

    #[test]
    fn the_demo_and_a_blank_score_are_valid() {
        assert_eq!(demo_score().validate(), Ok(()));
        assert_eq!(crate::blank_score("x").validate(), Ok(()));
    }

    #[test]
    fn a_zero_denominator_is_refused_instead_of_crashing_later() {
        let mut score = demo_score();
        score.tracks[0].measures[0].time_signature = TimeSignature {
            numerator: 4,
            denominator: 0,
        };
        assert!(error(&score).contains("unsupported time signature 4/0"));
    }

    #[test]
    fn an_incomplete_measure_names_where_it_is() {
        let mut score = demo_score();
        score.tracks[1].measures[2].voices[0].beats.pop();
        let message = error(&score);
        assert!(
            message.contains("Bass") && message.contains("measure 3"),
            "{message}"
        );
    }

    #[test]
    fn notes_must_be_on_existing_strings_and_frets() {
        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats[0].notes[0].string = 7;
        assert!(error(&score).contains("string 7"));

        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats[0].notes[0].fret = 40;
        assert!(error(&score).contains("fret 40"));
    }

    #[test]
    fn a_track_cannot_play_the_sound_of_another_instrument() {
        let mut score = demo_score();
        score.tracks[1].sound = Sound::ElectricGuitarDistortion;
        let message = error(&score);
        assert!(
            message.contains("Bass") && message.contains("electric-guitar/distortion"),
            "{message}"
        );
    }

    #[test]
    fn unsupported_durations_tempos_and_sizes_are_refused() {
        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats[0].duration = Duration {
            value: NoteValue::ThirtySecond,
            dots: 1,
        };
        assert!(error(&score).contains("duration"));

        let mut score = demo_score();
        score.tempo_bpm = 0;
        assert!(error(&score).contains("tempo"));

        let mut score = demo_score();
        score.tracks[0].tuning.clear();
        assert!(error(&score).contains("strings"));

        let mut score = demo_score();
        score.tracks[0].measures[0].voices.clear();
        assert!(error(&score).contains("no voice"));
    }
}
