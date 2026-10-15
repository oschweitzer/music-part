//! Checks that a score is safe to edit, display and play. Scores built by this program always
//! pass; the check is for files written by hand or by another version.

use std::fmt;

use crate::{NoteKind, Place, Score, Track, Voice, MAX_FRET, MAX_PITCH};

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
    /// signatures and durations, a sound of the track's instrument, notes of the kind of the
    /// instrument (on existing strings and frets, or at distinct pitches up to 127), and every
    /// voice filling its measure.
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
            let kind = track.instrument.note_kind();
            match kind {
                NoteKind::Fretted if !(1..=MAX_STRINGS).contains(&track.tuning.len()) => {
                    return fail(format!(
                        "track \"{name}\" must have between 1 and {MAX_STRINGS} strings"
                    ));
                }
                NoteKind::Pitched if !track.tuning.is_empty() => {
                    return fail(format!(
                        "track \"{name}\" has strings, but its instrument has none"
                    ));
                }
                _ => {}
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
                    check_notes(track, voice, &at)?;
                }
            }
        }
        Ok(())
    }
}

/// The notes of a voice are of the kind of the track, playable, and distinct within a beat.
fn check_notes(track: &Track, voice: &Voice, at: &str) -> Result<(), ScoreError> {
    let kind = track.instrument.note_kind();
    for beat in &voice.beats {
        for (i, note) in beat.notes.iter().enumerate() {
            if note.place.kind() != kind {
                let what = match kind {
                    NoteKind::Fretted => "a string and a fret",
                    NoteKind::Pitched => "a pitch",
                };
                return fail(format!(
                    "{at}: a note does not give {what}, as the instrument of the track needs"
                ));
            }
            match note.place {
                Place::Fretted { string, fret } => {
                    if string == 0 || usize::from(string) > track.tuning.len() {
                        return fail(format!(
                            "{at}: a note is on string {string}, which does not exist"
                        ));
                    }
                    if fret > MAX_FRET || track.pitch(note).is_none() {
                        return fail(format!(
                            "{at}: a note is on fret {fret}, which is out of range"
                        ));
                    }
                }
                Place::Pitched { pitch } => {
                    if pitch > MAX_PITCH {
                        return fail(format!(
                            "{at}: a note has the pitch {pitch}, above the highest MIDI pitch"
                        ));
                    }
                    if beat.notes[..i].iter().any(|n| n.key() == pitch) {
                        return fail(format!("{at}: a beat has the pitch {pitch} twice"));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        demo_score, Beat, Duration, Instrument, Measure, Note, NoteValue, Sound, TimeSignature,
    };

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
        score.tracks[0].measures[0].voices[0].beats[0].notes[0] = Note::fretted(7, 0);
        assert!(error(&score).contains("string 7"));

        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats[0].notes[0] = Note::fretted(1, 40);
        assert!(error(&score).contains("fret 40"));
    }

    /// A score with one piano track: one measure, a whole note at each pitch given.
    fn piano(pitches: &[u8]) -> Score {
        let mut score = crate::blank_score("p");
        let track = &mut score.tracks[0];
        track.instrument = Instrument::Piano;
        track.sound = Sound::Piano;
        track.tuning.clear();
        track.measures.truncate(1);
        track.measures[0] = Measure {
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            voices: vec![crate::Voice {
                beats: vec![Beat {
                    duration: Duration {
                        value: NoteValue::Whole,
                        dots: 0,
                    },
                    notes: pitches.iter().map(|&p| Note::pitched(p)).collect(),
                }],
            }],
        };
        score
    }

    #[test]
    fn a_piano_track_with_distinct_pitches_is_valid() {
        assert_eq!(piano(&[48, 60, 64, 67, 127]).validate(), Ok(()));
    }

    #[test]
    fn a_pitch_above_127_or_twice_in_a_beat_is_refused() {
        assert!(error(&piano(&[128])).contains("pitch 128"));
        assert!(error(&piano(&[60, 64, 60])).contains("pitch 60 twice"));
    }

    #[test]
    fn a_track_holds_only_notes_of_its_kind() {
        let mut score = piano(&[60]);
        score.tracks[0].measures[0].voices[0].beats[0].notes[0] = Note::fretted(1, 0);
        assert!(error(&score).contains("a pitch"), "{}", error(&score));

        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats[0].notes[0] = Note::pitched(60);
        assert!(error(&score).contains("a string and a fret"));
    }

    #[test]
    fn a_piano_has_no_strings() {
        let mut score = piano(&[60]);
        score.tracks[0].tuning = vec![64];
        assert!(error(&score).contains("has strings"));
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
