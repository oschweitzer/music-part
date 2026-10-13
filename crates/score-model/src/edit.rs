//! Editing a score. Every operation keeps the invariants checked by `Score::validate`: in
//! particular, a measure always stays exactly full. Lengthening a beat overwrites what follows it
//! in the measure; shortening one leaves a rest behind.
//!
//! Beats are addressed as (track, measure, beat) in the first voice.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    decompose_ticks, Beat, Duration, Measure, Note, Score, TimeSignature, Voice, MAX_FRET,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Edit {
    /// Puts a note on a string, replacing the one already there. A rest becomes a note.
    SetFret {
        track: usize,
        measure: usize,
        beat: usize,
        string: u8,
        fret: u8,
    },
    /// Removes the note on a string. A beat left without notes becomes a rest.
    RemoveNote {
        track: usize,
        measure: usize,
        beat: usize,
        string: u8,
    },
    SetDuration {
        track: usize,
        measure: usize,
        beat: usize,
        duration: Duration,
    },
    /// Inserts an empty measure before `at` in every track; `at` equal to the length appends.
    InsertMeasure {
        at: usize,
    },
    DeleteMeasure {
        index: usize,
    },
    SetTempo {
        bpm: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// Names what was out of range: "track", "measure", "beat" or "string".
    OutOfRange(&'static str),
    InvalidFret,
    InvalidDuration,
    InvalidTempo,
    /// The measure has no room left after the beat for the longer duration.
    NotEnoughRoom,
    LastMeasure,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange(what) => write!(f, "there is no such {what}"),
            Self::InvalidFret => write!(f, "the fret must be between 0 and {MAX_FRET}"),
            Self::InvalidDuration => write!(f, "this note duration is not supported"),
            Self::InvalidTempo => write!(f, "the tempo must be between 1 and 999 bpm"),
            Self::NotEnoughRoom => write!(f, "there is not enough room left in the measure"),
            Self::LastMeasure => write!(f, "a score needs at least one measure"),
        }
    }
}

impl std::error::Error for EditError {}

/// Applies an edit. On error the score is left exactly as it was.
pub fn apply_edit(score: &mut Score, edit: &Edit) -> Result<(), EditError> {
    match *edit {
        Edit::SetFret {
            track,
            measure,
            beat,
            string,
            fret,
        } => {
            if fret > MAX_FRET {
                return Err(EditError::InvalidFret);
            }
            let strings = score
                .tracks
                .get(track)
                .ok_or(EditError::OutOfRange("track"))?
                .tuning
                .len();
            if string == 0 || usize::from(string) > strings {
                return Err(EditError::OutOfRange("string"));
            }
            let beat = beat_mut(score, track, measure, beat)?;
            let note = Note {
                string,
                fret,
                technique: None,
                tied: false,
            };
            match beat.notes.iter_mut().find(|n| n.string == string) {
                Some(existing) => *existing = note,
                None => {
                    beat.notes.push(note);
                    beat.notes.sort_by_key(|n| n.string);
                }
            }
            Ok(())
        }
        Edit::RemoveNote {
            track,
            measure,
            beat,
            string,
        } => {
            beat_mut(score, track, measure, beat)?
                .notes
                .retain(|n| n.string != string);
            Ok(())
        }
        Edit::SetDuration {
            track,
            measure,
            beat,
            duration,
        } => {
            if !duration.is_valid() {
                return Err(EditError::InvalidDuration);
            }
            let beats = beats_mut(score, track, measure)?;
            set_duration(beats, beat, duration)
        }
        Edit::InsertMeasure { at } => insert_measure(score, at),
        Edit::DeleteMeasure { index } => delete_measure(score, index),
        Edit::SetTempo { bpm } => {
            if !(1..=999).contains(&bpm) {
                return Err(EditError::InvalidTempo);
            }
            score.tempo_bpm = bpm;
            Ok(())
        }
    }
}

fn beats_mut(score: &mut Score, track: usize, measure: usize) -> Result<&mut Vec<Beat>, EditError> {
    let track = score
        .tracks
        .get_mut(track)
        .ok_or(EditError::OutOfRange("track"))?;
    let measure = track
        .measures
        .get_mut(measure)
        .ok_or(EditError::OutOfRange("measure"))?;
    let voice = measure
        .voices
        .first_mut()
        .ok_or(EditError::OutOfRange("measure"))?;
    Ok(&mut voice.beats)
}

fn beat_mut(
    score: &mut Score,
    track: usize,
    measure: usize,
    beat: usize,
) -> Result<&mut Beat, EditError> {
    beats_mut(score, track, measure)?
        .get_mut(beat)
        .ok_or(EditError::OutOfRange("beat"))
}

fn rests(ticks: u32) -> Result<Vec<Beat>, EditError> {
    let durations = decompose_ticks(ticks).ok_or(EditError::InvalidDuration)?;
    Ok(durations
        .into_iter()
        .map(|duration| Beat {
            duration,
            notes: vec![],
        })
        .collect())
}

fn set_duration(beats: &mut Vec<Beat>, index: usize, duration: Duration) -> Result<(), EditError> {
    let old = beats
        .get(index)
        .ok_or(EditError::OutOfRange("beat"))?
        .duration
        .ticks();
    let new = duration.ticks();

    if new <= old {
        // Compute the filler first so a failure changes nothing.
        let filler = rests(old - new)?;
        beats[index].duration = duration;
        beats.splice(index + 1..index + 1, filler);
        return Ok(());
    }

    // Lengthening: swallow the following beats until there is enough room.
    let needed = new - old;
    let mut swallowed = 0;
    let mut end = index + 1;
    while swallowed < needed {
        let Some(next) = beats.get(end) else {
            return Err(EditError::NotEnoughRoom);
        };
        swallowed += next.duration.ticks();
        end += 1;
    }
    let filler = rests(swallowed - needed)?;
    beats.splice(index + 1..end, filler);
    beats[index].duration = duration;
    Ok(())
}

fn insert_measure(score: &mut Score, at: usize) -> Result<(), EditError> {
    let longest = score
        .tracks
        .iter()
        .map(|t| t.measures.len())
        .max()
        .unwrap_or(0);
    if at > longest {
        return Err(EditError::OutOfRange("measure"));
    }

    for track in &mut score.tracks {
        let at = at.min(track.measures.len());
        // The new measure copies the time signature of the one before it, or the one it displaces.
        let signature = at
            .checked_sub(1)
            .and_then(|i| track.measures.get(i))
            .or_else(|| track.measures.get(at))
            .map_or(
                TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                |m| m.time_signature,
            );
        let beats = rests(signature.ticks())?;
        track.measures.insert(
            at,
            Measure {
                time_signature: signature,
                voices: vec![Voice { beats }],
            },
        );
    }
    Ok(())
}

fn delete_measure(score: &mut Score, index: usize) -> Result<(), EditError> {
    let longest = score
        .tracks
        .iter()
        .map(|t| t.measures.len())
        .max()
        .unwrap_or(0);
    if index >= longest {
        return Err(EditError::OutOfRange("measure"));
    }
    if longest == 1 {
        return Err(EditError::LastMeasure);
    }
    for track in &mut score.tracks {
        if index < track.measures.len() {
            track.measures.remove(index);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{blank_score, demo_score, NoteValue};

    const fn dur(value: NoteValue, dots: u8) -> Duration {
        Duration { value, dots }
    }

    fn set_fret(measure: usize, beat: usize, string: u8, fret: u8) -> Edit {
        Edit::SetFret {
            track: 0,
            measure,
            beat,
            string,
            fret,
        }
    }

    fn set_duration_edit(measure: usize, beat: usize, duration: Duration) -> Edit {
        Edit::SetDuration {
            track: 0,
            measure,
            beat,
            duration,
        }
    }

    fn beats(score: &Score, measure: usize) -> &[Beat] {
        &score.tracks[0].measures[measure].voices[0].beats
    }

    fn lengths(score: &Score, measure: usize) -> Vec<u32> {
        beats(score, measure)
            .iter()
            .map(|b| b.duration.ticks())
            .collect()
    }

    #[test]
    fn a_fret_turns_a_rest_into_a_note() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 3, 5)).unwrap();

        let first = &beats(&score, 0)[0];
        assert_eq!(
            first.notes,
            vec![Note {
                string: 3,
                fret: 5,
                technique: None,
                tied: false
            }]
        );
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_fret_replaces_the_note_on_the_same_string_and_keeps_notes_sorted() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 5, 2)).unwrap();
        apply_edit(&mut score, &set_fret(0, 0, 1, 0)).unwrap();
        apply_edit(&mut score, &set_fret(0, 0, 5, 7)).unwrap();

        let notes: Vec<(u8, u8)> = beats(&score, 0)[0]
            .notes
            .iter()
            .map(|n| (n.string, n.fret))
            .collect();
        assert_eq!(notes, vec![(1, 0), (5, 7)]);
    }

    #[test]
    fn removing_the_last_note_leaves_a_rest() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 2, 3)).unwrap();
        apply_edit(
            &mut score,
            &Edit::RemoveNote {
                track: 0,
                measure: 0,
                beat: 0,
                string: 2,
            },
        )
        .unwrap();

        assert!(beats(&score, 0)[0].notes.is_empty());
        // Removing a note that is not there is not an error.
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::RemoveNote {
                    track: 0,
                    measure: 0,
                    beat: 0,
                    string: 2
                }
            ),
            Ok(())
        );
    }

    #[test]
    fn bad_addresses_and_values_are_refused_and_change_nothing() {
        let mut score = demo_score();
        let before = score.clone();
        let cases = [
            (set_fret(0, 0, 1, 25), EditError::InvalidFret),
            (set_fret(0, 0, 0, 3), EditError::OutOfRange("string")),
            (set_fret(0, 0, 7, 3), EditError::OutOfRange("string")),
            (set_fret(99, 0, 1, 3), EditError::OutOfRange("measure")),
            (set_fret(0, 99, 1, 3), EditError::OutOfRange("beat")),
            (
                Edit::SetFret {
                    track: 9,
                    measure: 0,
                    beat: 0,
                    string: 1,
                    fret: 0,
                },
                EditError::OutOfRange("track"),
            ),
            (
                set_duration_edit(0, 0, dur(NoteValue::ThirtySecond, 1)),
                EditError::InvalidDuration,
            ),
            (Edit::SetTempo { bpm: 0 }, EditError::InvalidTempo),
            (
                Edit::InsertMeasure { at: 99 },
                EditError::OutOfRange("measure"),
            ),
            (
                Edit::DeleteMeasure { index: 99 },
                EditError::OutOfRange("measure"),
            ),
        ];
        for (edit, expected) in cases {
            assert_eq!(apply_edit(&mut score, &edit), Err(expected), "{edit:?}");
            assert_eq!(score, before, "{edit:?} must not change the score");
        }
    }

    #[test]
    fn shortening_a_beat_leaves_a_rest_in_the_gap() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Quarter, 0)),
        )
        .unwrap();

        // Whole = 128: a quarter (32) and a dotted half (96) of rest.
        assert_eq!(lengths(&score, 0), vec![32, 96]);
        assert!(beats(&score, 0)[1].notes.is_empty());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn lengthening_a_beat_overwrites_what_follows() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Quarter, 0)),
        )
        .unwrap();
        apply_edit(&mut score, &set_fret(0, 0, 1, 0)).unwrap();
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Half, 0)),
        )
        .unwrap();

        // The quarter grew to a half; the 96 of rest after it lost 32 and became a half rest.
        assert_eq!(lengths(&score, 0), vec![64, 64]);
        assert_eq!(beats(&score, 0)[0].notes.len(), 1);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn lengthening_into_a_note_replaces_it() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Quarter, 0)),
        )
        .unwrap();
        apply_edit(
            &mut score,
            &set_duration_edit(0, 1, dur(NoteValue::Quarter, 0)),
        )
        .unwrap();
        apply_edit(&mut score, &set_fret(0, 1, 2, 4)).unwrap();
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Half, 0)),
        )
        .unwrap();

        assert!(beats(&score, 0).iter().all(|b| b.notes.is_empty()));
        assert_eq!(beats(&score, 0)[0].duration.ticks(), 64);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_beat_cannot_grow_past_the_end_of_its_measure() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Half, 0)),
        )
        .unwrap();
        // Beat 1 is the half-note rest that ends the measure: a whole note cannot fit there.
        let before = score.clone();
        assert_eq!(
            apply_edit(
                &mut score,
                &set_duration_edit(0, 1, dur(NoteValue::Whole, 0))
            ),
            Err(EditError::NotEnoughRoom)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn dotted_durations_work() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Quarter, 1)),
        )
        .unwrap();
        assert_eq!(lengths(&score, 0), vec![48, 64, 16]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_measure_is_inserted_in_every_track_with_the_neighbour_signature() {
        let mut score = demo_score();
        score.tracks[0].measures[1].time_signature = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        score.tracks[0].measures[1].voices[0].beats = rests(96).unwrap();
        let count = score.tracks[0].measures.len();

        apply_edit(&mut score, &Edit::InsertMeasure { at: 2 }).unwrap();

        assert_eq!(score.tracks[0].measures.len(), count + 1);
        assert_eq!(score.tracks[1].measures.len(), count + 1);
        assert_eq!(score.tracks[0].measures[2].time_signature.numerator, 3);
        assert_eq!(score.tracks[1].measures[2].time_signature.numerator, 4);
        assert_eq!(score.validate(), Ok(()));

        apply_edit(&mut score, &Edit::InsertMeasure { at: count + 1 }).unwrap();
        assert_eq!(score.tracks[0].measures.len(), count + 2);
    }

    #[test]
    fn a_measure_is_deleted_from_every_track_but_never_the_last() {
        let mut score = blank_score("t");
        for _ in 0..3 {
            apply_edit(&mut score, &Edit::DeleteMeasure { index: 0 }).unwrap();
        }
        assert_eq!(score.tracks[0].measures.len(), 1);
        assert_eq!(
            apply_edit(&mut score, &Edit::DeleteMeasure { index: 0 }),
            Err(EditError::LastMeasure)
        );
    }

    #[test]
    fn the_tempo_can_be_changed() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &Edit::SetTempo { bpm: 90 }).unwrap();
        assert_eq!(score.tempo_bpm, 90);
    }

    #[test]
    fn edits_travel_as_camel_case_json() {
        let edit: Edit = serde_json::from_str(
            r#"{"type":"setFret","track":0,"measure":1,"beat":2,"string":3,"fret":4}"#,
        )
        .unwrap();
        assert_eq!(edit, set_fret(1, 2, 3, 4));

        let edit: Edit = serde_json::from_str(
            r#"{"type":"setDuration","track":0,"measure":0,"beat":0,"duration":{"value":"Eighth","dots":1}}"#,
        )
        .unwrap();
        assert_eq!(edit, set_duration_edit(0, 0, dur(NoteValue::Eighth, 1)));
    }

    /// Whatever sequence of edits is thrown at a score, it stays valid, and a refused edit
    /// leaves it untouched.
    #[test]
    fn random_edits_never_break_a_score() {
        let mut seed = 12345u64;
        let mut next = move |below: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % below
        };
        let values = [
            NoteValue::Whole,
            NoteValue::Half,
            NoteValue::Quarter,
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
        ];

        for start in [demo_score(), blank_score("t")] {
            let mut score = start;
            let mut applied = 0;
            for step in 0..600 {
                // Mostly valid addresses, so that most edits are accepted; sometimes wild ones.
                let track = next(score.tracks.len());
                let measure = next(score.tracks[track].measures.len());
                let beat = next(score.tracks[track].measures[measure].voices[0].beats.len());
                let string = 1 + next(score.tracks[track].tuning.len()) as u8;
                let wild = next(12) == 0;
                let (measure, beat) = if wild {
                    (next(20), next(20))
                } else {
                    (measure, beat)
                };

                let edit = match next(7) {
                    0 | 1 => Edit::SetFret {
                        track,
                        measure,
                        beat,
                        string,
                        fret: next(if wild { 40 } else { 25 }) as u8,
                    },
                    2 => Edit::RemoveNote {
                        track,
                        measure,
                        beat,
                        string,
                    },
                    3 | 4 => Edit::SetDuration {
                        track,
                        measure,
                        beat,
                        duration: dur(values[next(6)], next(if wild { 4 } else { 3 }) as u8),
                    },
                    5 => Edit::InsertMeasure {
                        at: next(score.tracks[0].measures.len() + if wild { 8 } else { 1 }),
                    },
                    _ => Edit::DeleteMeasure {
                        index: next(score.tracks[0].measures.len() + if wild { 8 } else { 0 }),
                    },
                };
                let before = score.clone();
                match apply_edit(&mut score, &edit) {
                    Ok(()) => {
                        applied += 1;
                        assert_eq!(score.validate(), Ok(()), "step {step}: {edit:?}");
                    }
                    Err(_) => assert_eq!(score, before, "step {step}: {edit:?} changed the score"),
                }
            }
            assert!(
                applied > 100,
                "only {applied} edits applied: the test is not exercising much"
            );
        }
    }
}
