//! Editing a score. Every operation keeps the invariants checked by `Score::validate`: in
//! particular, a measure always stays exactly full. Lengthening a beat overwrites what follows it
//! in the measure; shortening one leaves a rest behind.
//!
//! Beats are addressed as (track, measure, beat) in the first voice.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    decompose_ticks, Beat, Duration, Instrument, Measure, Note, Score, Technique, TimeSignature,
    Track, Voice, MAX_FRET,
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
    /// Ties or unties a note to the same string of the previous beat (across barlines too). Tying
    /// copies the fret of the note it continues.
    SetTied {
        track: usize,
        measure: usize,
        beat: usize,
        string: u8,
        tied: bool,
    },
    /// Sets or clears the playing technique of a note.
    SetTechnique {
        track: usize,
        measure: usize,
        beat: usize,
        string: u8,
        technique: Option<Technique>,
    },
    /// Appends a track with the standard tuning, as many empty measures as the first track.
    AddTrack {
        instrument: Instrument,
    },
    RemoveTrack {
        index: usize,
    },
    /// Retunes a track, highest string first. Refused if a note would no longer be playable.
    SetTuning {
        track: usize,
        tuning: Vec<u8>,
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
    LastTrack,
    /// The note is not there, so it cannot be tied or given a technique.
    NoSuchNote,
    /// There is no note on the same string just before, to tie to.
    NothingToTie,
    InvalidTuning,
    TooManyTracks,
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
            Self::LastTrack => write!(f, "a score needs at least one track"),
            Self::NoSuchNote => write!(f, "there is no note there"),
            Self::NothingToTie => write!(f, "there is no note to tie to on this string"),
            Self::InvalidTuning => write!(
                f,
                "this tuning is not possible: it needs 1 to {MAX_STRINGS} strings, pitches up to 127, and every note must stay playable"
            ),
            Self::TooManyTracks => write!(f, "a score cannot have more than {MAX_TRACKS} tracks"),
        }
    }
}

impl std::error::Error for EditError {}

const MAX_STRINGS: usize = 12;
const MAX_TRACKS: usize = 64;

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
        Edit::SetTied {
            track,
            measure,
            beat,
            string,
            tied,
        } => set_tied(score, (track, measure, beat), string, tied),
        Edit::SetTechnique {
            track,
            measure,
            beat,
            string,
            technique,
        } => {
            note_mut(score, track, measure, beat, string)?.technique = technique;
            Ok(())
        }
        Edit::AddTrack { instrument } => add_track(score, instrument),
        Edit::RemoveTrack { index } => {
            if index >= score.tracks.len() {
                return Err(EditError::OutOfRange("track"));
            }
            if score.tracks.len() == 1 {
                return Err(EditError::LastTrack);
            }
            score.tracks.remove(index);
            Ok(())
        }
        Edit::SetTuning { track, ref tuning } => set_tuning(score, track, tuning),
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

fn note_mut(
    score: &mut Score,
    track: usize,
    measure: usize,
    beat: usize,
    string: u8,
) -> Result<&mut Note, EditError> {
    beat_mut(score, track, measure, beat)?
        .notes
        .iter_mut()
        .find(|n| n.string == string)
        .ok_or(EditError::NoSuchNote)
}

fn set_tied(
    score: &mut Score,
    (track, measure, beat): (usize, usize, usize),
    string: u8,
    tied: bool,
) -> Result<(), EditError> {
    // Fail on a bad address before looking for the previous note.
    note_mut(score, track, measure, beat, string)?;
    if !tied {
        note_mut(score, track, measure, beat, string)?.tied = false;
        return Ok(());
    }

    let measures = &score.tracks[track].measures;
    let previous = match beat.checked_sub(1) {
        Some(before) => measures[measure].voices[0].beats.get(before),
        None => measure
            .checked_sub(1)
            .and_then(|m| measures[m].voices.first())
            .and_then(|v| v.beats.last()),
    };
    let fret = previous
        .and_then(|b| b.notes.iter().find(|n| n.string == string))
        .map(|n| n.fret)
        .ok_or(EditError::NothingToTie)?;

    let note = note_mut(score, track, measure, beat, string)?;
    note.fret = fret;
    note.tied = true;
    Ok(())
}

fn add_track(score: &mut Score, instrument: Instrument) -> Result<(), EditError> {
    if score.tracks.len() >= MAX_TRACKS {
        return Err(EditError::TooManyTracks);
    }
    let signatures: Vec<TimeSignature> = match score.tracks.first() {
        Some(first) if !first.measures.is_empty() => {
            first.measures.iter().map(|m| m.time_signature).collect()
        }
        _ => vec![TimeSignature {
            numerator: 4,
            denominator: 4,
        }],
    };
    let measures = signatures
        .into_iter()
        .map(|time_signature| {
            Ok(Measure {
                time_signature,
                voices: vec![Voice {
                    beats: rests(time_signature.ticks())?,
                }],
            })
        })
        .collect::<Result<Vec<_>, EditError>>()?;

    let base = format!("{instrument:?}");
    let mut name = base.clone();
    let mut copy = 1;
    while score.tracks.iter().any(|t| t.name == name) {
        copy += 1;
        name = format!("{base} {copy}");
    }
    score.tracks.push(Track {
        name,
        instrument,
        tuning: instrument.standard_tuning().to_vec(),
        measures,
    });
    Ok(())
}

fn set_tuning(score: &mut Score, track: usize, tuning: &[u8]) -> Result<(), EditError> {
    let target = score
        .tracks
        .get_mut(track)
        .ok_or(EditError::OutOfRange("track"))?;
    if !(1..=MAX_STRINGS).contains(&tuning.len()) || tuning.iter().any(|&p| p > 127) {
        return Err(EditError::InvalidTuning);
    }
    let mut retuned = target.clone();
    retuned.tuning = tuning.to_vec();
    let is_playable = retuned
        .measures
        .iter()
        .flat_map(|m| &m.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
        .all(|n| retuned.pitch(n).is_some());
    if !is_playable {
        return Err(EditError::InvalidTuning);
    }
    *target = retuned;
    Ok(())
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
    use crate::{blank_score, demo_score, NoteValue, GUITAR_STANDARD};

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

    fn set_tied_edit(measure: usize, beat: usize, string: u8, tied: bool) -> Edit {
        Edit::SetTied {
            track: 0,
            measure,
            beat,
            string,
            tied,
        }
    }

    fn two_quarters_with(frets: [u8; 2]) -> Score {
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
        apply_edit(&mut score, &set_fret(0, 0, 2, frets[0])).unwrap();
        apply_edit(&mut score, &set_fret(0, 1, 2, frets[1])).unwrap();
        score
    }

    #[test]
    fn tying_copies_the_fret_of_the_note_before() {
        let mut score = two_quarters_with([5, 9]);
        apply_edit(&mut score, &set_tied_edit(0, 1, 2, true)).unwrap();

        let note = &beats(&score, 0)[1].notes[0];
        assert!(note.tied);
        assert_eq!(note.fret, 5);

        apply_edit(&mut score, &set_tied_edit(0, 1, 2, false)).unwrap();
        assert!(!beats(&score, 0)[1].notes[0].tied);
    }

    #[test]
    fn a_tie_can_reach_back_across_the_barline() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 3, 7)).unwrap();
        apply_edit(&mut score, &set_fret(1, 0, 3, 0)).unwrap();
        apply_edit(&mut score, &set_tied_edit(1, 0, 3, true)).unwrap();
        assert_eq!(beats(&score, 1)[0].notes[0].fret, 7);
    }

    #[test]
    fn a_tie_needs_a_note_on_the_same_string_just_before() {
        let mut score = two_quarters_with([5, 9]);
        apply_edit(&mut score, &set_fret(0, 1, 4, 1)).unwrap();
        let before = score.clone();

        // Nothing before the very first note of the score.
        assert_eq!(
            apply_edit(&mut score, &set_tied_edit(0, 0, 2, true)),
            Err(EditError::NothingToTie)
        );
        // The note before is on another string.
        assert_eq!(
            apply_edit(&mut score, &set_tied_edit(0, 1, 4, true)),
            Err(EditError::NothingToTie)
        );
        // There is no note to tie.
        assert_eq!(
            apply_edit(&mut score, &set_tied_edit(0, 1, 6, true)),
            Err(EditError::NoSuchNote)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_technique_is_set_and_cleared() {
        let mut score = two_quarters_with([5, 9]);
        let set = |technique| Edit::SetTechnique {
            track: 0,
            measure: 0,
            beat: 1,
            string: 2,
            technique,
        };
        apply_edit(&mut score, &set(Some(Technique::HammerOn))).unwrap();
        assert_eq!(
            beats(&score, 0)[1].notes[0].technique,
            Some(Technique::HammerOn)
        );
        apply_edit(&mut score, &set(None)).unwrap();
        assert_eq!(beats(&score, 0)[1].notes[0].technique, None);
    }

    #[test]
    fn a_track_is_added_with_matching_measures_and_a_unique_name() {
        let mut score = demo_score();
        let tracks = score.tracks.len();
        let measures = score.tracks[0].measures.len();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
            },
        )
        .unwrap();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
            },
        )
        .unwrap();

        assert_eq!(score.tracks.len(), tracks + 2);
        let added = &score.tracks[tracks];
        assert_eq!(added.measures.len(), measures);
        assert_eq!(added.tuning, Instrument::Bass.standard_tuning());
        assert_ne!(added.name, score.tracks[tracks + 1].name);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_track_is_removed_but_never_the_last_one() {
        let mut score = demo_score();
        apply_edit(&mut score, &Edit::RemoveTrack { index: 0 }).unwrap();
        assert_eq!(score.tracks.len(), 1);
        assert_eq!(
            apply_edit(&mut score, &Edit::RemoveTrack { index: 0 }),
            Err(EditError::LastTrack)
        );
        assert_eq!(
            apply_edit(&mut score, &Edit::RemoveTrack { index: 5 }),
            Err(EditError::OutOfRange("track"))
        );
    }

    #[test]
    fn a_track_can_be_retuned_unless_a_note_would_be_lost() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 6, 3)).unwrap();

        // Drop D: the lowest string goes down a tone.
        let mut drop_d = GUITAR_STANDARD.to_vec();
        drop_d[5] = 38;
        apply_edit(
            &mut score,
            &Edit::SetTuning {
                track: 0,
                tuning: drop_d.clone(),
            },
        )
        .unwrap();
        assert_eq!(score.tracks[0].tuning, drop_d);

        let before = score.clone();
        // Five strings: the note on string 6 would have nowhere to go.
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetTuning {
                    track: 0,
                    tuning: drop_d[..5].to_vec()
                }
            ),
            Err(EditError::InvalidTuning)
        );
        // Fret 3 on a string tuned to 127 is above the MIDI range.
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetTuning {
                    track: 0,
                    tuning: vec![64, 59, 55, 50, 45, 127]
                }
            ),
            Err(EditError::InvalidTuning)
        );
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetTuning {
                    track: 0,
                    tuning: vec![]
                }
            ),
            Err(EditError::InvalidTuning)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn new_edits_travel_as_camel_case_json() {
        let edit: Edit = serde_json::from_str(
            r#"{"type":"setTechnique","track":0,"measure":0,"beat":1,"string":2,"technique":"PalmMute"}"#,
        )
        .unwrap();
        assert!(matches!(
            edit,
            Edit::SetTechnique {
                technique: Some(Technique::PalmMute),
                ..
            }
        ));
        let edit: Edit =
            serde_json::from_str(r#"{"type":"addTrack","instrument":"Violin"}"#).unwrap();
        assert_eq!(
            edit,
            Edit::AddTrack {
                instrument: Instrument::Violin
            }
        );
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

                let edit = match next(12) {
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
                    6 => Edit::DeleteMeasure {
                        index: next(score.tracks[0].measures.len() + if wild { 8 } else { 0 }),
                    },
                    7 => Edit::SetTied {
                        track,
                        measure,
                        beat,
                        string,
                        tied: next(3) != 0,
                    },
                    8 => Edit::SetTechnique {
                        track,
                        measure,
                        beat,
                        string,
                        technique: [None, Some(Technique::Slide), Some(Technique::Bend)][next(3)],
                    },
                    9 => Edit::AddTrack {
                        instrument: [Instrument::Guitar, Instrument::Bass, Instrument::Violin]
                            [next(3)],
                    },
                    10 => Edit::RemoveTrack {
                        index: next(score.tracks.len() + 1),
                    },
                    _ => Edit::SetTuning {
                        track,
                        tuning: (0..1 + next(if wild { 14 } else { 7 }))
                            .map(|i| 80 - 5 * i as u8 + next(if wild { 80 } else { 3 }) as u8)
                            .collect(),
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
