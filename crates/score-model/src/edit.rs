//! Editing a score. Every operation keeps the invariants checked by `Score::validate`: in
//! particular, a measure always stays exactly full. Lengthening a beat overwrites what follows it
//! in the measure; shortening one leaves a rest behind.
//!
//! Beats are addressed as (track, measure, beat) in the first voice. A note is addressed by its
//! string on a fretted track (`SetFret`, `RemoveNote`, `SetTied`...) and by the pitch it sounds
//! on any track (`AddPitch`, `RemovePitch`, `MovePitch`, `SetPitchTied`), which is how a staff
//! shows it. On a drum track that "pitch" is the General MIDI key of the piece (`Track::pitch`).
//! See `docs/notation.md` and `docs/drums.md`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    decompose_ticks, fret_pitch, pitch_fret, place_chord, place_pitch, Beat, Duration, Instrument,
    Measure, Note, NoteKind, Place, Score, Sound, Technique, TimeSignature, Track, Voice,
    DRUM_PIECES, MAX_FRET, MAX_PITCH,
};

/// A beat of the first voice of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeatAddress {
    pub measure: usize,
    pub beat: usize,
}

/// The strings from `first` to `last`, both included (1 is the highest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringSpan {
    pub first: u8,
    pub last: u8,
}

/// The MIDI pitches from `low` to `high`, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PitchSpan {
    pub low: u8,
    pub high: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Edit {
    /// Puts a note on a string, replacing the one already there. A rest becomes a note. Refused on
    /// a track without strings.
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
    /// Deletes `count` measures from `index` on, in every track. The last measure stays.
    DeleteMeasure {
        index: usize,
        #[serde(default = "one")]
        count: usize,
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
    /// Appends a track with the standard tuning, as many empty measures as the first track, and
    /// `sound` (the default sound of the instrument when not given). Refused if the sound is not
    /// one of the instrument.
    AddTrack {
        instrument: Instrument,
        #[serde(default)]
        sound: Option<Sound>,
    },
    RemoveTrack {
        index: usize,
    },
    /// Writes `beats` one after the other starting at a beat, over what is there: the paste of the
    /// copy-and-paste, and with rests the clearing of a range. Pasted beats are cut at barlines
    /// (the cut part is tied on) and fitted into the measures they land on. Notes on strings the
    /// track does not have are left out. Refused if the score ends before the paste does.
    Paste {
        track: usize,
        measure: usize,
        beat: usize,
        beats: Vec<Beat>,
    },
    /// Changes the time signature of a measure and of the measures after it that have the same
    /// one. Notes that no longer fit are cut short (the cut part is tied on) or dropped at the end
    /// of the measure; a longer measure is filled with rests.
    SetTimeSignature {
        measure: usize,
        numerator: u8,
        denominator: u8,
    },
    RenameTrack {
        track: usize,
        name: String,
    },
    /// Changes the instrument of a track; its sound becomes the default sound of the new
    /// instrument. Between two fretted instruments the strings and frets of the notes are left
    /// alone (retune the track if the strings differ); refused if a note would land on a fret the
    /// instrument does not have (the first frets of the short fifth string of a banjo). From a
    /// fretted to a pitched instrument every note keeps its pitch and its tie, without technique.
    /// From a pitched to a fretted one the track gets the standard tuning and the pitches are
    /// placed on the strings; refused if a note cannot be. To or from drums only a track without
    /// notes changes (a drum piece is no pitch): refused otherwise.
    SetInstrument {
        track: usize,
        instrument: Instrument,
    },
    /// Changes the sound of a track, among the sounds of its instrument.
    SetSound {
        track: usize,
        sound: Sound,
    },
    /// Retunes a track, highest string first. Refused if a note would no longer be playable, and on
    /// a track without strings.
    SetTuning {
        track: usize,
        tuning: Vec<u8>,
    },
    /// Removes the notes of the beats from `from` to `to` (across barlines), only those on
    /// `strings`, or sounding within `pitches`, when given. The rhythm stays: a beat left without
    /// notes is a rest.
    ClearNotes {
        track: usize,
        from: BeatAddress,
        to: BeatAddress,
        strings: Option<StringSpan>,
        #[serde(default)]
        pitches: Option<PitchSpan>,
    },
    /// Adds a note sounding at a MIDI pitch. On a pitched track it joins the notes of the beat (a
    /// chord); on a fretted track it goes on a free string, at the lowest fret that plays it
    /// (refused if none can); on a drum track the pitch is the key of a piece (refused if it is
    /// not 35 to 81). A rest becomes a note; a pitch the beat already has changes nothing.
    AddPitch {
        track: usize,
        measure: usize,
        beat: usize,
        pitch: u8,
    },
    /// Removes the note sounding at a pitch. A beat left without notes becomes a rest.
    RemovePitch {
        track: usize,
        measure: usize,
        beat: usize,
        pitch: u8,
    },
    /// Moves the note sounding at `pitch` to the pitch `to`, keeping its technique. It is no
    /// longer tied, and neither is the note that continued it. On a fretted track it stays on its
    /// string when the string reaches `to`, else it goes to a free one. On a drum track it moves
    /// to the piece `to`. Refused if the beat already has `to`.
    MovePitch {
        track: usize,
        measure: usize,
        beat: usize,
        pitch: u8,
        to: u8,
    },
    /// Ties or unties the note sounding at a pitch to the same pitch on the previous beat (across
    /// barlines too). On a fretted track it ties the note's string, as `SetTied` does.
    SetPitchTied {
        track: usize,
        measure: usize,
        beat: usize,
        pitch: u8,
        tied: bool,
    },
    /// Takes the rests from `from` to `to` out of the timeline of every track: what follows moves
    /// earlier and the measure gets shorter (its time signature follows), or disappears when it
    /// has no time left. Refused unless every track has only rests over that time, so no note is
    /// ever lost.
    RemoveRests {
        track: usize,
        from: BeatAddress,
        to: BeatAddress,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// Names what was out of range: "track", "measure", "beat" or "string".
    OutOfRange(&'static str),
    InvalidFret,
    /// The string has no such fret: the first frets of the short fifth string of a banjo, or a
    /// fret above MIDI pitch 127.
    NoSuchFret,
    /// The sound belongs to another instrument than the track's.
    SoundOfAnotherInstrument,
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
    InvalidTimeSignature,
    /// The name is empty or longer than `MAX_NAME_CHARS`.
    InvalidName,
    /// Only rests can be removed from the timeline.
    NotARest,
    /// Another track has a note, or a beat that cannot be cut, in the time to remove.
    RestInUse,
    /// The track has no strings: its notes are pitches.
    NoStrings,
    /// A MIDI pitch is 0 to 127.
    InvalidPitch,
    /// The beat already has a note at this pitch.
    PitchTaken,
    /// No free string of the instrument can play the pitch.
    NoFreeString,
    /// A drum piece is a General MIDI percussion key from 35 to 81.
    InvalidPiece,
    /// The track has notes, and the new instrument stores another kind of note with no way to
    /// convert them (drum pieces and pitches).
    NotesOfAnotherKind,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange(what) => write!(f, "there is no such {what}"),
            Self::InvalidFret => write!(f, "the fret must be between 0 and {MAX_FRET}"),
            Self::NoSuchFret => write!(
                f,
                "this string has no such fret (the short fifth string of a banjo starts at fret 6)"
            ),
            Self::SoundOfAnotherInstrument => {
                write!(f, "this sound belongs to another instrument")
            }
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
            Self::InvalidTimeSignature => write!(
                f,
                "this time signature is not supported: the denominator must be 1, 2, 4, 8, 16 or 32 and a measure must be a whole number of 32nd notes"
            ),
            Self::InvalidName => write!(f, "the name must be 1 to {MAX_NAME_CHARS} characters"),
            Self::NotARest => write!(f, "only a rest can be removed: clear its notes first"),
            Self::RestInUse => write!(
                f,
                "another track has notes at this place, so the rest cannot be removed"
            ),
            Self::TooManyTracks => write!(f, "a score cannot have more than {MAX_TRACKS} tracks"),
            Self::NoStrings => write!(
                f,
                "this track has no strings: write its notes on the staff"
            ),
            Self::InvalidPitch => write!(f, "the pitch must be between 0 and {MAX_PITCH}"),
            Self::PitchTaken => write!(f, "this beat already has a note at this pitch"),
            Self::NoFreeString => {
                write!(f, "no free string of this instrument can play this pitch")
            }
            Self::InvalidPiece => write!(
                f,
                "a drum piece is a General MIDI percussion key from 35 to 81"
            ),
            Self::NotesOfAnotherKind => write!(
                f,
                "drum pieces are not pitches: clear the notes of this track before changing between drums and another instrument"
            ),
        }
    }
}

impl std::error::Error for EditError {}

const MAX_STRINGS: usize = 12;
const MAX_TRACKS: usize = 64;
const MAX_NAME_CHARS: usize = 64;

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
            let target = score
                .tracks
                .get(track)
                .ok_or(EditError::OutOfRange("track"))?;
            if target.instrument.note_kind() != NoteKind::Fretted {
                return Err(EditError::NoStrings);
            }
            if string == 0 || usize::from(string) > target.tuning.len() {
                return Err(EditError::OutOfRange("string"));
            }
            let note = Note::fretted(string, fret);
            // A very high tuning can leave no MIDI pitch for a fret that is otherwise allowed, and
            // a banjo's short string has no first frets.
            if score.tracks[track].pitch(&note).is_none() {
                return Err(EditError::NoSuchFret);
            }
            let beat = beat_mut(score, track, measure, beat)?;
            match beat.notes.iter_mut().find(|n| n.string() == Some(string)) {
                Some(existing) => *existing = note,
                None => {
                    beat.notes.push(note);
                    beat.notes.sort_by_key(Note::key);
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
                .retain(|n| n.string() != Some(string));
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
        Edit::ClearNotes {
            track,
            from,
            to,
            strings,
            pitches,
        } => clear_notes(score, track, (from, to), strings, pitches),
        Edit::AddPitch {
            track,
            measure,
            beat,
            pitch,
        } => add_pitch(score, (track, measure, beat), pitch),
        Edit::RemovePitch {
            track,
            measure,
            beat,
            pitch,
        } => {
            let target = score
                .tracks
                .get(track)
                .ok_or(EditError::OutOfRange("track"))?;
            let keys = keys_sounding(target, beat_of(target, measure, beat)?, pitch);
            beat_mut(score, track, measure, beat)?
                .notes
                .retain(|n| !keys.contains(&n.key()));
            Ok(())
        }
        Edit::MovePitch {
            track,
            measure,
            beat,
            pitch,
            to,
        } => move_pitch(score, (track, measure, beat), pitch, to),
        Edit::SetPitchTied {
            track,
            measure,
            beat,
            pitch,
            tied,
        } => set_pitch_tied(score, (track, measure, beat), pitch, tied),
        Edit::RemoveRests { track, from, to } => remove_rests(score, track, from, to),
        Edit::InsertMeasure { at } => insert_measure(score, at),
        Edit::DeleteMeasure { index, count } => delete_measures(score, index, count),
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
        Edit::Paste {
            track,
            measure,
            beat,
            ref beats,
        } => paste(score, (track, measure, beat), beats),
        Edit::SetTimeSignature {
            measure,
            numerator,
            denominator,
        } => set_time_signature(
            score,
            measure,
            TimeSignature {
                numerator,
                denominator,
            },
        ),
        Edit::RenameTrack { track, ref name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
                return Err(EditError::InvalidName);
            }
            score
                .tracks
                .get_mut(track)
                .ok_or(EditError::OutOfRange("track"))?
                .name = name.to_string();
            Ok(())
        }
        Edit::SetInstrument { track, instrument } => set_instrument(score, track, instrument),
        Edit::SetSound { track, sound } => {
            let target = score
                .tracks
                .get_mut(track)
                .ok_or(EditError::OutOfRange("track"))?;
            if sound.instrument() != target.instrument {
                return Err(EditError::SoundOfAnotherInstrument);
            }
            target.sound = sound;
            Ok(())
        }
        Edit::AddTrack { instrument, sound } => {
            let sound = sound.unwrap_or_else(|| instrument.default_sound());
            if sound.instrument() != instrument {
                return Err(EditError::SoundOfAnotherInstrument);
            }
            add_track(score, sound)
        }
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

/// A beat of the first voice of a track.
fn beat_of(track: &Track, measure: usize, beat: usize) -> Result<&Beat, EditError> {
    track
        .measures
        .get(measure)
        .ok_or(EditError::OutOfRange("measure"))?
        .voices
        .first()
        .ok_or(EditError::OutOfRange("measure"))?
        .beats
        .get(beat)
        .ok_or(EditError::OutOfRange("beat"))
}

/// The keys (`Note::key`) of the notes of a beat that sound at `pitch`.
fn keys_sounding(track: &Track, beat: &Beat, pitch: u8) -> Vec<u8> {
    beat.notes
        .iter()
        .filter(|n| track.pitch(n) == Some(pitch))
        .map(Note::key)
        .collect()
}

/// The beat before one in the first voice: the last of the previous measure for a first beat.
fn previous_beat(track: &Track, measure: usize, beat: usize) -> Option<&Beat> {
    match beat.checked_sub(1) {
        Some(before) => track
            .measures
            .get(measure)?
            .voices
            .first()?
            .beats
            .get(before),
        None => track
            .measures
            .get(measure.checked_sub(1)?)?
            .voices
            .first()?
            .beats
            .last(),
    }
}

/// The (measure, beat) after one in the first voice: the first of the next measure after a last
/// beat.
fn next_address(track: &Track, measure: usize, beat: usize) -> Option<(usize, usize)> {
    let count = track.measures.get(measure)?.voices.first()?.beats.len();
    if beat + 1 < count {
        return Some((measure, beat + 1));
    }
    let next = track.measures.get(measure + 1)?.voices.first()?;
    (!next.beats.is_empty()).then_some((measure + 1, 0))
}

fn add_pitch(
    score: &mut Score,
    (track, measure, beat): (usize, usize, usize),
    pitch: u8,
) -> Result<(), EditError> {
    if pitch > MAX_PITCH {
        return Err(EditError::InvalidPitch);
    }
    let target = score
        .tracks
        .get(track)
        .ok_or(EditError::OutOfRange("track"))?;
    let current = beat_of(target, measure, beat)?;
    if current.notes.iter().any(|n| target.pitch(n) == Some(pitch)) {
        return Ok(());
    }
    let note = match target.instrument.note_kind() {
        NoteKind::Pitched => Note::pitched(pitch),
        NoteKind::Percussion if DRUM_PIECES.contains(&pitch) => Note::percussion(pitch),
        NoteKind::Percussion => return Err(EditError::InvalidPiece),
        NoteKind::Fretted => {
            let taken: Vec<u8> = current.notes.iter().map(Note::key).collect();
            let (string, fret) = place_pitch(target.instrument, &target.tuning, pitch, &taken)
                .ok_or(EditError::NoFreeString)?;
            Note::fretted(string, fret)
        }
    };
    let notes = &mut beat_mut(score, track, measure, beat)?.notes;
    notes.push(note);
    notes.sort_by_key(Note::key);
    Ok(())
}

fn move_pitch(
    score: &mut Score,
    (track, measure, beat): (usize, usize, usize),
    pitch: u8,
    to: u8,
) -> Result<(), EditError> {
    if to > MAX_PITCH {
        return Err(EditError::InvalidPitch);
    }
    let target = score
        .tracks
        .get(track)
        .ok_or(EditError::OutOfRange("track"))?;
    let current = beat_of(target, measure, beat)?;
    let index = current
        .notes
        .iter()
        .position(|n| target.pitch(n) == Some(pitch))
        .ok_or(EditError::NoSuchNote)?;
    if to == pitch {
        return Ok(());
    }
    if current.notes.iter().any(|n| target.pitch(n) == Some(to)) {
        return Err(EditError::PitchTaken);
    }
    let place = match current.notes[index].place {
        Place::Pitched { .. } => Place::Pitched { pitch: to },
        Place::Percussion { .. } if DRUM_PIECES.contains(&to) => Place::Percussion { piece: to },
        Place::Percussion { .. } => return Err(EditError::InvalidPiece),
        Place::Fretted { string, .. } => {
            let (instrument, tuning) = (target.instrument, &target.tuning);
            let taken: Vec<u8> = current
                .notes
                .iter()
                .map(Note::key)
                .filter(|s| *s != string)
                .collect();
            let (string, fret) = pitch_fret(instrument, tuning, string, to)
                .map(|fret| (string, fret))
                .or_else(|| place_pitch(instrument, tuning, to, &taken))
                .ok_or(EditError::NoFreeString)?;
            Place::Fretted { string, fret }
        }
    };
    // The note that continued this one no longer has anything to continue.
    let continuation = next_address(target, measure, beat).map(|(m, b)| {
        let next = &target.measures[m].voices[0].beats[b];
        (m, b, keys_sounding(target, next, pitch))
    });

    let notes = &mut beat_mut(score, track, measure, beat)?.notes;
    notes[index].place = place;
    notes[index].tied = false;
    notes.sort_by_key(Note::key);
    if let Some((m, b, keys)) = continuation {
        for note in &mut beat_mut(score, track, m, b)?.notes {
            if keys.contains(&note.key()) {
                note.tied = false;
            }
        }
    }
    Ok(())
}

fn set_pitch_tied(
    score: &mut Score,
    (track, measure, beat): (usize, usize, usize),
    pitch: u8,
    tied: bool,
) -> Result<(), EditError> {
    let target = score
        .tracks
        .get(track)
        .ok_or(EditError::OutOfRange("track"))?;
    let note = beat_of(target, measure, beat)?
        .notes
        .iter()
        .find(|n| target.pitch(n) == Some(pitch))
        .ok_or(EditError::NoSuchNote)?;
    if let Some(string) = note.string() {
        return set_tied(score, (track, measure, beat), string, tied);
    }
    let is_continued = previous_beat(target, measure, beat)
        .is_some_and(|b| b.notes.iter().any(|n| target.pitch(n) == Some(pitch)));
    if tied && !is_continued {
        return Err(EditError::NothingToTie);
    }
    let key = note.key();
    for note in &mut beat_mut(score, track, measure, beat)?.notes {
        if note.key() == key {
            note.tied = tied;
        }
    }
    Ok(())
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
        .find(|n| n.string() == Some(string))
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

    let fret = previous_beat(&score.tracks[track], measure, beat)
        .and_then(|b| b.notes.iter().find(|n| n.string() == Some(string)))
        .and_then(Note::fret)
        .ok_or(EditError::NothingToTie)?;

    let note = note_mut(score, track, measure, beat, string)?;
    note.place = Place::Fretted { string, fret };
    note.tied = true;
    Ok(())
}

fn paste(
    score: &mut Score,
    (track, measure, beat): (usize, usize, usize),
    pasted: &[Beat],
) -> Result<(), EditError> {
    let target = score
        .tracks
        .get(track)
        .ok_or(EditError::OutOfRange("track"))?;
    if pasted.iter().any(|b| !b.duration.is_valid()) {
        return Err(EditError::InvalidDuration);
    }
    if pasted
        .iter()
        .flat_map(|b| &b.notes)
        .any(|n| n.fret().is_some_and(|f| f > MAX_FRET))
    {
        return Err(EditError::InvalidFret);
    }

    // Where the paste starts, from the start of the piece.
    let first = target
        .measures
        .get(measure)
        .and_then(|m| m.voices.first())
        .ok_or(EditError::OutOfRange("measure"))?;
    if beat >= first.beats.len() {
        return Err(EditError::OutOfRange("beat"));
    }
    let measure_starts: Vec<u32> = target
        .measures
        .iter()
        .scan(0, |at, m| {
            let start = *at;
            *at += m.time_signature.ticks();
            Some(start)
        })
        .collect();
    let start = measure_starts[measure]
        + first.beats[..beat]
            .iter()
            .map(|b| b.duration.ticks())
            .sum::<u32>();
    let length: u32 = pasted.iter().map(|b| b.duration.ticks()).sum();
    let end = start + length;
    let score_end = measure_starts.last().copied().unwrap_or(0)
        + target
            .measures
            .last()
            .map_or(0, |m| m.time_signature.ticks());
    if end > score_end {
        return Err(EditError::NotEnoughRoom);
    }
    if length == 0 {
        return Ok(());
    }

    // The pasted beats on the timeline of the piece, without the notes the track cannot hold:
    // on strings it does not have, or of another kind (fretted notes on a piano), and without
    // the techniques of drum notes (they are for strings).
    let strings = target.tuning.len();
    let kind = target.instrument.note_kind();
    let mut at = start;
    let placed: Vec<(u32, u32, Beat)> = pasted
        .iter()
        .map(|b| {
            let mut beat = b.clone();
            beat.notes.retain(|n| {
                n.place.kind() == kind
                    && n.string()
                        .is_none_or(|s| s >= 1 && usize::from(s) <= strings)
            });
            if kind == NoteKind::Percussion {
                for note in &mut beat.notes {
                    note.technique = None;
                }
            }
            beat.notes.sort_by_key(Note::key);
            beat.notes.dedup_by_key(|n| n.key());
            let from = at;
            at += b.duration.ticks();
            (from, at, beat)
        })
        .collect();

    if placed
        .iter()
        .flat_map(|(_, _, b)| &b.notes)
        .any(|n| target.pitch(n).is_none())
    {
        return Err(match kind {
            NoteKind::Percussion => EditError::InvalidPiece,
            _ => EditError::InvalidFret,
        });
    }

    let mut rebuilt: Vec<(usize, Vec<Beat>)> = Vec::new();
    for (m, existing) in target.measures.iter().enumerate() {
        let m_start = measure_starts[m];
        let m_end = m_start + existing.time_signature.ticks();
        if m_end <= start || m_start >= end {
            continue;
        }
        let Some(voice) = existing.voices.first() else {
            continue;
        };
        let (lo, hi) = (start.max(m_start) - m_start, end.min(m_end) - m_start);

        let mut out: Vec<Beat> = Vec::new();
        let mut position = 0;
        let mut after: Vec<Beat> = Vec::new();
        for old in &voice.beats {
            let old_end = position + old.duration.ticks();
            if position < lo {
                if old_end <= lo {
                    out.push(old.clone());
                } else {
                    out.extend(split_beat(old, lo - position)?);
                }
            }
            if old_end > hi {
                if position >= hi {
                    after.push(old.clone());
                } else {
                    after.extend(rests(old_end - hi)?);
                }
            }
            position = old_end;
        }

        for (from, to, piece) in &placed {
            let (a, b) = ((*from).max(m_start + lo), (*to).min(m_start + hi));
            if a >= b {
                continue;
            }
            let mut piece = piece.clone();
            if a > *from {
                // This is the continuation of a beat that began in the previous measure.
                for note in &mut piece.notes {
                    note.tied = true;
                }
            }
            out.extend(split_beat(&piece, b - a)?);
        }
        out.extend(after);
        rebuilt.push((m, out));
    }

    let target = &mut score.tracks[track];
    for (m, beats) in rebuilt {
        target.measures[m].voices[0].beats = beats;
    }
    Ok(())
}

fn set_time_signature(
    score: &mut Score,
    measure: usize,
    signature: TimeSignature,
) -> Result<(), EditError> {
    if !signature.is_valid() {
        return Err(EditError::InvalidTimeSignature);
    }
    let longest = score
        .tracks
        .iter()
        .map(|t| t.measures.len())
        .max()
        .unwrap_or(0);
    if measure >= longest {
        return Err(EditError::OutOfRange("measure"));
    }

    // Build everything first so that a failure changes nothing.
    let mut rebuilt: Vec<(usize, usize, Measure)> = Vec::new();
    for (t, track) in score.tracks.iter().enumerate() {
        let Some(reference) = track.measures.get(measure) else {
            continue;
        };
        let old = reference.time_signature;
        for (m, existing) in track.measures.iter().enumerate().skip(measure) {
            if existing.time_signature != old {
                break;
            }
            let mut resized = existing.clone();
            resized.time_signature = signature;
            for voice in &mut resized.voices {
                voice.beats = refit(&voice.beats, signature.ticks())?;
            }
            rebuilt.push((t, m, resized));
        }
    }
    for (t, m, resized) in rebuilt {
        score.tracks[t].measures[m] = resized;
    }
    Ok(())
}

/// The beats cut or padded with rests to last exactly `ticks`. A beat that crosses the end is
/// shortened; what is cut off is dropped.
fn refit(beats: &[Beat], ticks: u32) -> Result<Vec<Beat>, EditError> {
    let mut out: Vec<Beat> = Vec::new();
    let mut position = 0;
    for beat in beats {
        if position >= ticks {
            break;
        }
        let end = position + beat.duration.ticks();
        if end <= ticks {
            out.push(beat.clone());
        } else {
            out.extend(split_beat(beat, ticks - position)?);
        }
        position = end.min(ticks);
    }
    if position < ticks {
        out.extend(rests(ticks - position)?);
    }
    Ok(out)
}

/// A beat as several shorter beats adding up to `ticks`; the notes are repeated, tied, on the
/// ones after the first so the whole still sounds for the same time.
fn split_beat(beat: &Beat, ticks: u32) -> Result<Vec<Beat>, EditError> {
    let durations = decompose_ticks(ticks).ok_or(EditError::InvalidDuration)?;
    Ok(durations
        .into_iter()
        .enumerate()
        .map(|(i, duration)| Beat {
            duration,
            notes: beat
                .notes
                .iter()
                .map(|n| Note {
                    tied: n.tied || i > 0,
                    ..n.clone()
                })
                .collect(),
        })
        .collect())
}

fn set_instrument(
    score: &mut Score,
    track: usize,
    instrument: Instrument,
) -> Result<(), EditError> {
    let target = score
        .tracks
        .get_mut(track)
        .ok_or(EditError::OutOfRange("track"))?;
    if target.instrument == instrument {
        return Ok(());
    }
    let mut changed = target.clone();
    changed.instrument = instrument;
    changed.sound = instrument.default_sound();
    match (target.instrument.note_kind(), instrument.note_kind()) {
        (NoteKind::Fretted, NoteKind::Fretted) => {
            let is_playable = changed
                .measures
                .iter()
                .flat_map(|m| &m.voices)
                .flat_map(|v| &v.beats)
                .flat_map(|b| &b.notes)
                .all(|n| changed.pitch(n).is_some());
            if !is_playable {
                return Err(EditError::NoSuchFret);
            }
        }
        (NoteKind::Fretted, NoteKind::Pitched) => {
            changed.tuning.clear();
            for beat in beats_of_mut(&mut changed) {
                let mut notes = Vec::with_capacity(beat.notes.len());
                for note in &beat.notes {
                    let pitch = target.pitch(note).ok_or(EditError::NoSuchFret)?;
                    notes.push(Note {
                        tied: note.tied,
                        ..Note::pitched(pitch)
                    });
                }
                notes.sort_by_key(Note::key);
                // Two strings at the same pitch become one note.
                notes.dedup_by_key(|n| n.key());
                beat.notes = notes;
            }
        }
        (NoteKind::Pitched, NoteKind::Fretted) => {
            let tuning = instrument.standard_tuning();
            changed.tuning = tuning.to_vec();
            for beat in beats_of_mut(&mut changed) {
                let pitches: Vec<u8> = beat.notes.iter().map(Note::key).collect();
                let (mut placed, skipped) = place_chord(instrument, tuning, &pitches, &[]);
                if skipped > 0 {
                    return Err(EditError::NoFreeString);
                }
                for note in &mut placed {
                    let pitch = note
                        .fret()
                        .and_then(|fret| fret_pitch(instrument, tuning, note.key(), fret));
                    note.tied = beat.notes.iter().any(|n| Some(n.key()) == pitch && n.tied);
                }
                beat.notes = placed;
            }
        }
        (NoteKind::Pitched, NoteKind::Pitched) | (NoteKind::Percussion, NoteKind::Percussion) => {}
        (NoteKind::Percussion, _) | (_, NoteKind::Percussion) => {
            if beats_of_mut(&mut changed).any(|b| !b.notes.is_empty()) {
                return Err(EditError::NotesOfAnotherKind);
            }
            changed.tuning = instrument.standard_tuning().to_vec();
        }
    }
    *target = changed;
    Ok(())
}

/// Every beat of a track, in every voice.
fn beats_of_mut(track: &mut Track) -> impl Iterator<Item = &mut Beat> {
    track
        .measures
        .iter_mut()
        .flat_map(|m| &mut m.voices)
        .flat_map(|v| &mut v.beats)
}

fn add_track(score: &mut Score, sound: Sound) -> Result<(), EditError> {
    let instrument = sound.instrument();
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
        sound,
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
    if target.instrument.note_kind() != NoteKind::Fretted {
        return Err(EditError::NoStrings);
    }
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

fn one() -> usize {
    1
}

/// The (measure, beat) of every beat from `from` to `to` in the first voice, in order.
fn beats_between(
    track: &Track,
    from: BeatAddress,
    to: BeatAddress,
) -> Result<Vec<(usize, usize)>, EditError> {
    let count = |measure: usize| {
        track
            .measures
            .get(measure)
            .and_then(|m| m.voices.first())
            .map(|v| v.beats.len())
    };
    for address in [from, to] {
        let beats = count(address.measure).ok_or(EditError::OutOfRange("measure"))?;
        if address.beat >= beats {
            return Err(EditError::OutOfRange("beat"));
        }
    }
    if (from.measure, from.beat) > (to.measure, to.beat) {
        return Err(EditError::OutOfRange("beat"));
    }

    let mut out = Vec::new();
    for measure in from.measure..=to.measure {
        let beats = count(measure).ok_or(EditError::OutOfRange("measure"))?;
        let first = if measure == from.measure {
            from.beat
        } else {
            0
        };
        let last = if measure == to.measure {
            to.beat
        } else {
            beats - 1
        };
        out.extend((first..=last).map(|beat| (measure, beat)));
    }
    Ok(out)
}

fn clear_notes(
    score: &mut Score,
    track: usize,
    (from, to): (BeatAddress, BeatAddress),
    strings: Option<StringSpan>,
    pitches: Option<PitchSpan>,
) -> Result<(), EditError> {
    let target = score
        .tracks
        .get_mut(track)
        .ok_or(EditError::OutOfRange("track"))?;
    if let Some(span) = strings {
        if span.first == 0 || span.first > span.last || usize::from(span.last) > target.tuning.len()
        {
            return Err(EditError::OutOfRange("string"));
        }
    }
    if pitches.is_some_and(|span| span.low > span.high || span.high > MAX_PITCH) {
        return Err(EditError::InvalidPitch);
    }
    let beats = beats_between(target, from, to)?;
    for (measure, beat) in beats {
        let kept: Vec<Note> = target.measures[measure].voices[0].beats[beat]
            .notes
            .iter()
            .filter(|n| {
                let is_off_strings = strings.is_some_and(|s| {
                    n.string()
                        .is_none_or(|string| string < s.first || string > s.last)
                });
                let is_off_pitches = pitches.is_some_and(|span| {
                    target
                        .pitch(n)
                        .is_none_or(|p| p < span.low || p > span.high)
                });
                is_off_strings || is_off_pitches
            })
            .cloned()
            .collect();
        target.measures[measure].voices[0].beats[beat].notes = kept;
    }
    Ok(())
}

/// The time signature for a measure of `ticks`: the largest note value that divides it, so a
/// 4/4 measure that loses a quarter becomes 3/4 and one that loses an eighth becomes 7/8.
fn time_signature_for(ticks: u32) -> Option<TimeSignature> {
    [1u32, 2, 4, 8, 16, 32].into_iter().find_map(|denominator| {
        let whole = ticks * denominator;
        let signature = TimeSignature {
            numerator: u8::try_from(whole / 128).ok()?,
            denominator: u8::try_from(denominator).ok()?,
        };
        (whole.is_multiple_of(128) && signature.is_valid()).then_some(signature)
    })
}

/// Takes the time `from..to` (ticks from the start of the measure) out of a voice made of rests
/// over that time. A beat the span cuts into keeps what is left of it, as rests.
fn cut_span(beats: &[Beat], from: u32, to: u32) -> Result<Vec<Beat>, EditError> {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let mut touched: Option<(u32, u32)> = None;
    let mut at = 0;
    for beat in beats {
        let (start, end) = (at, at + beat.duration.ticks());
        at = end;
        if end <= from {
            before.push(beat.clone());
        } else if start >= to {
            after.push(beat.clone());
        } else if beat.notes.is_empty() {
            let (first, _) = touched.unwrap_or((start, end));
            touched = Some((first, end));
        } else {
            return Err(EditError::RestInUse);
        }
    }
    let (first, last) = touched.ok_or(EditError::RestInUse)?;
    let left = (last - first) - (to - from);
    let rests = decompose_ticks(left).ok_or(EditError::RestInUse)?;
    before.extend(rests.into_iter().map(|duration| Beat {
        duration,
        notes: Vec::new(),
    }));
    before.extend(after);
    Ok(before)
}

fn remove_rests(
    score: &mut Score,
    track: usize,
    from: BeatAddress,
    to: BeatAddress,
) -> Result<(), EditError> {
    let source = score
        .tracks
        .get(track)
        .ok_or(EditError::OutOfRange("track"))?;
    let beats = beats_between(source, from, to)?;

    // For each measure, the time of the chosen beats: ticks from its start.
    let mut spans: Vec<(usize, u32, u32)> = Vec::new();
    for measure in from.measure..=to.measure {
        let chosen: Vec<usize> = beats
            .iter()
            .filter(|(m, _)| *m == measure)
            .map(|(_, b)| *b)
            .collect();
        let voice = &source.measures[measure].voices[0];
        if chosen.iter().any(|&b| !voice.beats[b].notes.is_empty()) {
            return Err(EditError::NotARest);
        }
        let ticks = |range: &[Beat]| range.iter().map(|b| b.duration.ticks()).sum::<u32>();
        let start = ticks(&voice.beats[..chosen[0]]);
        let end = start + ticks(&voice.beats[chosen[0]..=chosen[chosen.len() - 1]]);
        spans.push((measure, start, end));
    }

    let mut edited = score.clone();
    let mut emptied = Vec::new();
    for (measure, start, end) in spans {
        let old = source.measures[measure].time_signature;
        let remaining = old.ticks() - (end - start);
        let signature = time_signature_for(remaining);
        if remaining > 0 && signature.is_none() {
            return Err(EditError::RestInUse);
        }
        for other in &mut edited.tracks {
            let current = other
                .measures
                .get_mut(measure)
                .ok_or(EditError::RestInUse)?;
            // Tracks are kept the same length; one that is not cannot be cut in step.
            if current.time_signature != old {
                return Err(EditError::RestInUse);
            }
            for voice in &mut current.voices {
                voice.beats = if remaining == 0 {
                    // Checked below: the whole measure has to be rests in every voice.
                    cut_span(&voice.beats, start, end)?;
                    Vec::new()
                } else {
                    cut_span(&voice.beats, start, end)?
                };
            }
            if let Some(signature) = signature {
                current.time_signature = signature;
            }
        }
        if remaining == 0 {
            emptied.push(measure);
        }
    }

    if !emptied.is_empty() {
        let measures = edited.tracks.first().map_or(0, |t| t.measures.len());
        if emptied.len() >= measures {
            return Err(EditError::LastMeasure);
        }
        for track in &mut edited.tracks {
            for &measure in emptied.iter().rev() {
                track.measures.remove(measure);
            }
        }
    }
    *score = edited;
    Ok(())
}

fn delete_measures(score: &mut Score, index: usize, count: usize) -> Result<(), EditError> {
    let longest = score
        .tracks
        .iter()
        .map(|t| t.measures.len())
        .max()
        .unwrap_or(0);
    let end = index.saturating_add(count);
    if count == 0 || end > longest {
        return Err(EditError::OutOfRange("measure"));
    }
    if count == longest {
        return Err(EditError::LastMeasure);
    }
    for track in &mut score.tracks {
        let from = index.min(track.measures.len());
        track.measures.drain(from..end.min(track.measures.len()));
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
        assert_eq!(first.notes, vec![Note::fretted(3, 5)]);
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
            .map(|n| (n.key(), n.fret().unwrap_or(0)))
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
                Edit::DeleteMeasure {
                    index: 99,
                    count: 1,
                },
                EditError::OutOfRange("measure"),
            ),
            (
                Edit::DeleteMeasure { index: 0, count: 0 },
                EditError::OutOfRange("measure"),
            ),
            (
                Edit::DeleteMeasure {
                    index: usize::MAX,
                    count: 2,
                },
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
            apply_edit(&mut score, &Edit::DeleteMeasure { index: 0, count: 1 }).unwrap();
        }
        assert_eq!(score.tracks[0].measures.len(), 1);
        assert_eq!(
            apply_edit(&mut score, &Edit::DeleteMeasure { index: 0, count: 1 }),
            Err(EditError::LastMeasure)
        );
    }

    #[test]
    fn several_measures_are_deleted_at_once_from_every_track() {
        let mut score = demo_score();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
                sound: None,
            },
        )
        .unwrap();
        let before: Vec<usize> = score.tracks.iter().map(|t| t.measures.len()).collect();
        let kept_last = score.tracks[0].measures.last().cloned();

        apply_edit(&mut score, &Edit::DeleteMeasure { index: 1, count: 2 }).unwrap();

        let after: Vec<usize> = score.tracks.iter().map(|t| t.measures.len()).collect();
        assert_eq!(after, before.iter().map(|n| n - 2).collect::<Vec<_>>());
        assert_eq!(score.tracks[0].measures.last().cloned(), kept_last);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn deleting_every_measure_is_refused_and_changes_nothing() {
        let mut score = demo_score();
        let count = score.tracks[0].measures.len();
        let before = score.clone();

        assert_eq!(
            apply_edit(&mut score, &Edit::DeleteMeasure { index: 0, count }),
            Err(EditError::LastMeasure)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_measure_deletion_without_a_count_deletes_one_measure() {
        let edit: Edit = serde_json::from_str(r#"{"type":"deleteMeasure","index":3}"#).unwrap();
        assert_eq!(edit, Edit::DeleteMeasure { index: 3, count: 1 });
    }

    fn at(measure: usize, beat: usize) -> BeatAddress {
        BeatAddress { measure, beat }
    }

    fn clear(from: BeatAddress, to: BeatAddress, strings: Option<(u8, u8)>) -> Edit {
        Edit::ClearNotes {
            track: 0,
            from,
            to,
            strings: strings.map(|(first, last)| StringSpan { first, last }),
            pitches: None,
        }
    }

    /// A first measure of four quarters: a note, a rest, a note, a rest.
    fn note_rest_note_rest() -> Score {
        let mut score = blank_score("t");
        let clip = vec![
            note_beat(NoteValue::Quarter, &[(1, 3)]),
            note_beat(NoteValue::Quarter, &[]),
            note_beat(NoteValue::Quarter, &[(2, 5)]),
            note_beat(NoteValue::Quarter, &[]),
        ];
        apply_edit(&mut score, &paste_edit(0, 0, 0, clip)).unwrap();
        score
    }

    #[test]
    fn clearing_notes_keeps_the_strings_outside_the_span() {
        let mut score = blank_score("t");
        let chord = note_beat(NoteValue::Whole, &[(1, 0), (2, 1), (3, 2), (4, 3)]);
        apply_edit(&mut score, &paste_edit(0, 0, 0, vec![chord])).unwrap();

        apply_edit(&mut score, &clear(at(0, 0), at(0, 0), Some((2, 3)))).unwrap();

        assert_eq!(frets_of(&score, 0), vec![vec![(1, 0), (4, 3)]]);
        assert_eq!(lengths(&score, 0), vec![128]);
    }

    #[test]
    fn clearing_without_a_span_empties_the_beats_across_a_barline() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 1, 5)).unwrap();
        apply_edit(&mut score, &set_fret(1, 0, 2, 7)).unwrap();

        apply_edit(&mut score, &clear(at(0, 0), at(1, 0), None)).unwrap();

        assert!(beats(&score, 0)[0].notes.is_empty() && beats(&score, 1)[0].notes.is_empty());
    }

    #[test]
    fn clearing_refuses_a_bad_address_or_span_and_changes_nothing() {
        let mut score = note_rest_note_rest();
        let before = score.clone();
        for edit in [
            clear(at(0, 3), at(0, 0), None),
            clear(at(0, 0), at(0, 9), None),
            clear(at(9, 0), at(9, 0), None),
            clear(at(0, 0), at(0, 1), Some((0, 2))),
            clear(at(0, 0), at(0, 1), Some((3, 2))),
            clear(at(0, 0), at(0, 1), Some((1, 7))),
        ] {
            assert!(apply_edit(&mut score, &edit).is_err(), "{edit:?}");
            assert_eq!(score, before);
        }
    }

    fn remove_rest(from: BeatAddress, to: BeatAddress) -> Edit {
        Edit::RemoveRests { track: 0, from, to }
    }

    #[test]
    fn removing_a_rest_closes_the_gap_and_shortens_the_measure() {
        let mut score = note_rest_note_rest();
        apply_edit(&mut score, &remove_rest(at(0, 1), at(0, 1))).unwrap();

        assert_eq!(frets_of(&score, 0)[..2], [vec![(1, 3)], vec![(2, 5)]]);
        assert_eq!(lengths(&score, 0), vec![32, 32, 32]);
        let signature = score.tracks[0].measures[0].time_signature;
        assert_eq!((signature.numerator, signature.denominator), (3, 4));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_rest_shorter_than_a_beat_leaves_a_measure_in_eighths() {
        let mut score = blank_score("t");
        let clip = vec![
            note_beat(NoteValue::Half, &[(1, 0)]),
            note_beat(NoteValue::Quarter, &[]),
            note_beat(NoteValue::Eighth, &[(1, 2)]),
            note_beat(NoteValue::Eighth, &[]),
        ];
        apply_edit(&mut score, &paste_edit(0, 0, 0, clip)).unwrap();
        apply_edit(&mut score, &remove_rest(at(0, 3), at(0, 3))).unwrap();

        let signature = score.tracks[0].measures[0].time_signature;
        assert_eq!((signature.numerator, signature.denominator), (7, 8));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn removing_the_only_rest_of_a_measure_deletes_the_measure_in_every_track() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
                sound: None,
            },
        )
        .unwrap();
        let measures = score.tracks[0].measures.len();

        apply_edit(&mut score, &remove_rest(at(1, 0), at(1, 0))).unwrap();

        assert!(score
            .tracks
            .iter()
            .all(|t| t.measures.len() == measures - 1));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn removing_a_rest_is_refused_while_another_track_plays_there() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
                sound: None,
            },
        )
        .unwrap();
        apply_edit(
            &mut score,
            &Edit::SetFret {
                track: 1,
                measure: 1,
                beat: 0,
                string: 1,
                fret: 3,
            },
        )
        .unwrap();
        let before = score.clone();

        assert_eq!(
            apply_edit(&mut score, &remove_rest(at(1, 0), at(1, 0))),
            Err(EditError::RestInUse)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn removing_a_rest_works_when_another_track_has_a_longer_rest_there() {
        let mut score = note_rest_note_rest();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
                sound: None,
            },
        )
        .unwrap();

        // The bass has a whole rest; the guitar's second quarter goes out of it.
        apply_edit(&mut score, &remove_rest(at(0, 1), at(0, 1))).unwrap();

        let bass: u32 = score.tracks[1].measures[0].voices[0]
            .beats
            .iter()
            .map(|b| b.duration.ticks())
            .sum();
        assert_eq!(bass, 96);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn removing_a_beat_with_notes_is_refused() {
        let mut score = note_rest_note_rest();
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &remove_rest(at(0, 0), at(0, 1))),
            Err(EditError::NotARest)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn the_last_measure_of_the_score_cannot_be_removed_as_a_rest() {
        let mut score = blank_score("t");
        let count = score.tracks[0].measures.len();
        apply_edit(&mut score, &remove_rest(at(0, 0), at(count - 2, 0))).unwrap();
        assert_eq!(score.tracks[0].measures.len(), 1);

        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &remove_rest(at(0, 0), at(0, 0))),
            Err(EditError::LastMeasure)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn rests_of_several_measures_are_removed_in_one_edit() {
        let mut score = blank_score("t");
        let count = score.tracks[0].measures.len();
        apply_edit(&mut score, &remove_rest(at(1, 0), at(2, 0))).unwrap();
        assert_eq!(score.tracks[0].measures.len(), count - 2);
        assert_eq!(score.validate(), Ok(()));
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
        assert_eq!(note.fret(), Some(5));

        apply_edit(&mut score, &set_tied_edit(0, 1, 2, false)).unwrap();
        assert!(!beats(&score, 0)[1].notes[0].tied);
    }

    #[test]
    fn a_tie_can_reach_back_across_the_barline() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 3, 7)).unwrap();
        apply_edit(&mut score, &set_fret(1, 0, 3, 0)).unwrap();
        apply_edit(&mut score, &set_tied_edit(1, 0, 3, true)).unwrap();
        assert_eq!(beats(&score, 1)[0].notes[0].fret(), Some(7));
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
                sound: None,
            },
        )
        .unwrap();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Bass,
                sound: None,
            },
        )
        .unwrap();

        assert_eq!(score.tracks.len(), tracks + 2);
        let added = &score.tracks[tracks];
        assert_eq!(added.measures.len(), measures);
        assert_eq!(added.tuning, Instrument::Bass.standard_tuning());
        assert_eq!(added.sound, Sound::BassFinger);
        assert_ne!(added.name, score.tracks[tracks + 1].name);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_track_is_added_with_the_sound_asked_for_if_its_instrument_has_it() {
        let mut score = demo_score();
        let add = |sound| Edit::AddTrack {
            instrument: Instrument::ElectricGuitar,
            sound: Some(sound),
        };
        apply_edit(&mut score, &add(Sound::ElectricGuitarDistortion)).unwrap();
        assert_eq!(
            score.tracks.last().unwrap().sound,
            Sound::ElectricGuitarDistortion
        );

        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &add(Sound::BassSlap)),
            Err(EditError::SoundOfAnotherInstrument)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_banjo_track_gets_five_strings_and_the_banjo_sound() {
        let mut score = demo_score();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Banjo,
                sound: None,
            },
        )
        .unwrap();
        let banjo = score.tracks.last().unwrap();
        assert_eq!(banjo.tuning, crate::BANJO_STANDARD);
        assert_eq!(banjo.sound, Sound::Banjo);
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
    fn a_fret_above_the_midi_range_on_a_very_high_string_is_refused() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &Edit::SetTuning {
                track: 0,
                tuning: vec![120, 59, 55, 50, 45, 40],
            },
        )
        .unwrap();
        let before = score.clone();
        // 120 + 10 is above MIDI pitch 127.
        assert_eq!(
            apply_edit(&mut score, &set_fret(0, 0, 1, 10)),
            Err(EditError::NoSuchFret)
        );
        assert_eq!(score, before);
        apply_edit(&mut score, &set_fret(0, 0, 1, 7)).unwrap();
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
                instrument: Instrument::Violin,
                sound: None,
            }
        );
        let edit: Edit =
            serde_json::from_str(r#"{"type":"setSound","track":1,"sound":"bass/slap"}"#).unwrap();
        assert_eq!(
            edit,
            Edit::SetSound {
                track: 1,
                sound: Sound::BassSlap
            }
        );
    }

    fn signature_edit(measure: usize, numerator: u8, denominator: u8) -> Edit {
        Edit::SetTimeSignature {
            measure,
            numerator,
            denominator,
        }
    }

    #[test]
    fn a_shorter_measure_cuts_the_notes_that_no_longer_fit() {
        let mut score = blank_score("t");
        // One whole note on string 3, fret 5, in a 4/4 measure; make it 3/4: a dotted half.
        apply_edit(&mut score, &set_fret(0, 0, 3, 5)).unwrap();
        apply_edit(&mut score, &signature_edit(0, 3, 4)).unwrap();
        assert_eq!(score.tracks[0].measures[0].time_signature.numerator, 3);
        assert_eq!(lengths(&score, 0), vec![96]);
        assert_eq!(beats(&score, 0)[0].notes[0].fret(), Some(5));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_cut_that_needs_two_durations_ties_the_second_part() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 3, 5)).unwrap();
        // 5/8 is 80 ticks: a half note and a sixteenth.
        apply_edit(&mut score, &signature_edit(0, 5, 8)).unwrap();

        assert_eq!(lengths(&score, 0), vec![64, 16]);
        let notes: Vec<(u8, bool)> = beats(&score, 0)
            .iter()
            .map(|b| (b.notes[0].fret().unwrap_or(0), b.notes[0].tied))
            .collect();
        assert_eq!(notes, vec![(5, false), (5, true)]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_longer_measure_is_filled_with_rests() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 1, 0)).unwrap();
        apply_edit(&mut score, &signature_edit(0, 6, 4)).unwrap();

        assert_eq!(lengths(&score, 0), vec![128, 64]);
        assert!(beats(&score, 0)[1].notes.is_empty());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_new_signature_carries_on_until_a_measure_that_already_differed() {
        let mut score = blank_score("t"); // four 4/4 measures
        apply_edit(&mut score, &signature_edit(2, 3, 4)).unwrap();
        // Measures 2 and 3 are 3/4 now. Changing measure 0 stops at the 3/4 ones.
        apply_edit(&mut score, &signature_edit(0, 2, 4)).unwrap();

        let numerators: Vec<u8> = score.tracks[0]
            .measures
            .iter()
            .map(|m| m.time_signature.numerator)
            .collect();
        assert_eq!(numerators, vec![2, 2, 3, 3]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_time_signature_applies_to_every_track() {
        let mut score = demo_score();
        apply_edit(&mut score, &signature_edit(0, 3, 4)).unwrap();
        assert!(score
            .tracks
            .iter()
            .all(|t| t.measures[0].time_signature.numerator == 3));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn unsupported_signatures_are_refused_and_change_nothing() {
        let mut score = demo_score();
        let before = score.clone();
        for (numerator, denominator) in [(0, 4), (4, 0), (4, 3), (4, 64), (1, 32 + 1)] {
            assert_eq!(
                apply_edit(&mut score, &signature_edit(0, numerator, denominator)),
                Err(EditError::InvalidTimeSignature),
                "{numerator}/{denominator}"
            );
        }
        assert_eq!(
            apply_edit(&mut score, &signature_edit(99, 4, 4)),
            Err(EditError::OutOfRange("measure"))
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_track_is_renamed_with_a_trimmed_non_empty_name() {
        let mut score = demo_score();
        let rename = |name: &str| Edit::RenameTrack {
            track: 0,
            name: name.to_string(),
        };
        apply_edit(&mut score, &rename("  Lead  ")).unwrap();
        assert_eq!(score.tracks[0].name, "Lead");
        assert_eq!(
            apply_edit(&mut score, &rename("   ")),
            Err(EditError::InvalidName)
        );
        assert_eq!(
            apply_edit(&mut score, &rename(&"x".repeat(65))),
            Err(EditError::InvalidName)
        );
        assert_eq!(score.tracks[0].name, "Lead");
    }

    #[test]
    fn the_instrument_of_a_track_can_change_without_touching_its_strings() {
        let mut score = demo_score();
        let tuning = score.tracks[0].tuning.clone();
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Violin,
            },
        )
        .unwrap();
        assert_eq!(score.tracks[0].instrument, Instrument::Violin);
        assert_eq!(score.tracks[0].tuning, tuning);
    }

    #[test]
    fn a_new_instrument_brings_its_default_sound() {
        let mut score = demo_score();
        score.tracks[0].instrument = Instrument::ElectricGuitar;
        score.tracks[0].sound = Sound::ElectricGuitarDistortion;
        let set = |instrument| Edit::SetInstrument {
            track: 0,
            instrument,
        };

        apply_edit(&mut score, &set(Instrument::ElectricGuitar)).unwrap();
        assert_eq!(score.tracks[0].sound, Sound::ElectricGuitarDistortion);

        apply_edit(&mut score, &set(Instrument::Guitar)).unwrap();
        assert_eq!(score.tracks[0].sound, Sound::GuitarNylon);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn an_instrument_is_refused_if_a_note_would_be_on_a_fret_it_lacks() {
        let mut score = demo_score();
        // Five strings, with a note at fret 2 of the fifth: a banjo's fifth string has no fret 2.
        score.tracks[0].tuning.truncate(5);
        score.tracks[0].measures[0].voices[0].beats[0].notes = vec![Note::fretted(5, 2)];
        let before = score.clone();
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetInstrument {
                    track: 0,
                    instrument: Instrument::Banjo,
                }
            ),
            Err(EditError::NoSuchFret)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn the_sound_of_a_track_changes_among_those_of_its_instrument() {
        let mut score = demo_score();
        let set = |sound| Edit::SetSound { track: 1, sound };

        apply_edit(&mut score, &set(Sound::BassSlap)).unwrap();
        assert_eq!(score.tracks[1].sound, Sound::BassSlap);
        assert_eq!(score.tracks[1].instrument, Instrument::Bass);

        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &set(Sound::Violin)),
            Err(EditError::SoundOfAnotherInstrument)
        );
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetSound {
                    track: 9,
                    sound: Sound::BassPick
                }
            ),
            Err(EditError::OutOfRange("track"))
        );
        assert_eq!(score, before);
    }

    #[test]
    fn the_first_frets_of_the_short_banjo_string_are_refused() {
        let mut score = demo_score();
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Banjo,
                sound: None,
            },
        )
        .unwrap();
        let banjo = score.tracks.len() - 1;
        let fret = |fret| Edit::SetFret {
            track: banjo,
            measure: 0,
            beat: 0,
            string: 5,
            fret,
        };
        assert_eq!(apply_edit(&mut score, &fret(3)), Err(EditError::NoSuchFret));
        apply_edit(&mut score, &fret(0)).unwrap();
        apply_edit(&mut score, &fret(7)).unwrap();
        assert_eq!(score.validate(), Ok(()));
    }

    fn note_beat(value: NoteValue, notes: &[(u8, u8)]) -> Beat {
        Beat {
            duration: dur(value, 0),
            notes: notes
                .iter()
                .map(|&(string, fret)| Note::fretted(string, fret))
                .collect(),
        }
    }

    fn paste_edit(track: usize, measure: usize, beat: usize, beats: Vec<Beat>) -> Edit {
        Edit::Paste {
            track,
            measure,
            beat,
            beats,
        }
    }

    fn frets_of(score: &Score, measure: usize) -> Vec<Vec<(u8, u8)>> {
        beats(score, measure)
            .iter()
            .map(|b| {
                b.notes
                    .iter()
                    .map(|n| (n.key(), n.fret().unwrap_or(0)))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn pasted_beats_are_written_over_the_start_of_a_measure() {
        let mut score = blank_score("t");
        let clip = vec![
            note_beat(NoteValue::Eighth, &[(2, 3)]),
            note_beat(NoteValue::Eighth, &[(2, 5), (4, 0)]),
        ];
        apply_edit(&mut score, &paste_edit(0, 0, 0, clip)).unwrap();

        assert_eq!(lengths(&score, 0), vec![16, 16, 96]);
        assert_eq!(
            frets_of(&score, 0),
            vec![vec![(2, 3)], vec![(2, 5), (4, 0)], vec![]]
        );
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_paste_replaces_the_notes_it_lands_on_and_rests_for_what_it_shortens() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 1, 7)).unwrap(); // a whole note
        apply_edit(
            &mut score,
            &paste_edit(0, 0, 0, vec![note_beat(NoteValue::Quarter, &[(2, 1)])]),
        )
        .unwrap();

        // The quarter replaced the start of the whole note; the other three quarters are rests.
        assert_eq!(lengths(&score, 0), vec![32, 96]);
        assert_eq!(frets_of(&score, 0), vec![vec![(2, 1)], vec![]]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_paste_that_crosses_a_barline_is_cut_and_tied_on() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &set_duration_edit(0, 0, dur(NoteValue::Half, 0)),
        )
        .unwrap();
        // Measure 0 is now two half-note rests: paste a whole note on the second one.
        apply_edit(
            &mut score,
            &paste_edit(0, 0, 1, vec![note_beat(NoteValue::Whole, &[(3, 5)])]),
        )
        .unwrap();

        assert_eq!(lengths(&score, 0), vec![64, 64]);
        assert_eq!(frets_of(&score, 0), vec![vec![], vec![(3, 5)]]);
        assert_eq!(lengths(&score, 1), vec![64, 64]);
        assert_eq!(frets_of(&score, 1), vec![vec![(3, 5)], vec![]]);
        assert!(beats(&score, 1)[0].notes[0].tied);
        assert!(!beats(&score, 0)[1].notes[0].tied);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn notes_on_strings_the_track_does_not_have_are_left_out() {
        let mut score = demo_score(); // track 1 is a 4-string bass
        apply_edit(
            &mut score,
            &paste_edit(
                1,
                0,
                0,
                vec![note_beat(NoteValue::Quarter, &[(2, 3), (6, 0)])],
            ),
        )
        .unwrap();

        let first = &score.tracks[1].measures[0].voices[0].beats[0];
        assert_eq!(
            first.notes.iter().map(Note::key).collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn rests_clear_a_range() {
        let mut score = demo_score();
        let clear: Vec<Beat> = beats(&score, 0)
            .iter()
            .map(|b| Beat {
                duration: b.duration,
                notes: vec![],
            })
            .collect();
        apply_edit(&mut score, &paste_edit(0, 0, 0, clear)).unwrap();

        assert!(beats(&score, 0).iter().all(|b| b.notes.is_empty()));
        assert!(!beats(&score, 1).iter().all(|b| b.notes.is_empty()));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_paste_that_runs_past_the_end_of_the_score_is_refused_whole() {
        let mut score = blank_score("t"); // four whole-note measures
        let before = score.clone();
        let clip = vec![note_beat(NoteValue::Whole, &[(1, 1)]); 3];
        assert_eq!(
            apply_edit(&mut score, &paste_edit(0, 2, 0, clip)),
            Err(EditError::NotEnoughRoom)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_paste_with_a_bad_fret_duration_or_address_is_refused() {
        let mut score = blank_score("t");
        let before = score.clone();
        let bad_duration = Beat {
            duration: dur(NoteValue::ThirtySecond, 1),
            notes: vec![],
        };
        for (edit, expected) in [
            (
                paste_edit(0, 0, 0, vec![note_beat(NoteValue::Quarter, &[(1, 30)])]),
                EditError::InvalidFret,
            ),
            (
                paste_edit(0, 0, 0, vec![bad_duration]),
                EditError::InvalidDuration,
            ),
            (
                paste_edit(0, 9, 0, vec![note_beat(NoteValue::Quarter, &[])]),
                EditError::OutOfRange("measure"),
            ),
            (
                paste_edit(0, 0, 9, vec![note_beat(NoteValue::Quarter, &[])]),
                EditError::OutOfRange("beat"),
            ),
            (
                paste_edit(5, 0, 0, vec![note_beat(NoteValue::Quarter, &[])]),
                EditError::OutOfRange("track"),
            ),
        ] {
            assert_eq!(apply_edit(&mut score, &edit), Err(expected), "{edit:?}");
        }
        assert_eq!(score, before);
        // Pasting nothing is allowed and changes nothing.
        apply_edit(&mut score, &paste_edit(0, 0, 0, vec![])).unwrap();
        assert_eq!(score, before);
    }

    /// A blank score whose track is a piano: four measures of whole rests.
    fn piano_score() -> Score {
        let mut score = blank_score("p");
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Piano,
            },
        )
        .unwrap();
        score
    }

    fn add_pitch(measure: usize, beat: usize, pitch: u8) -> Edit {
        Edit::AddPitch {
            track: 0,
            measure,
            beat,
            pitch,
        }
    }

    fn move_pitch(measure: usize, beat: usize, pitch: u8, to: u8) -> Edit {
        Edit::MovePitch {
            track: 0,
            measure,
            beat,
            pitch,
            to,
        }
    }

    fn pitches_of(score: &Score, measure: usize, beat: usize) -> Vec<u8> {
        beats(score, measure)[beat]
            .notes
            .iter()
            .filter_map(|n| score.tracks[0].pitch(n))
            .collect()
    }

    /// The first measure as two halves, the first holding `first`, the second `second`.
    fn two_halves(score: &mut Score, first: &[u8], second: &[u8]) {
        apply_edit(score, &set_duration_edit(0, 0, dur(NoteValue::Half, 0))).unwrap();
        for &pitch in first {
            apply_edit(score, &add_pitch(0, 0, pitch)).unwrap();
        }
        for &pitch in second {
            apply_edit(score, &add_pitch(0, 1, pitch)).unwrap();
        }
    }

    #[test]
    fn a_pitch_turns_a_rest_of_a_piano_into_a_note() {
        let mut score = piano_score();
        apply_edit(&mut score, &add_pitch(0, 0, 60)).unwrap();
        assert_eq!(beats(&score, 0)[0].notes, vec![Note::pitched(60)]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn pitches_added_to_a_beat_make_a_sorted_chord_without_duplicates() {
        let mut score = piano_score();
        for pitch in [67, 60, 64, 60] {
            apply_edit(&mut score, &add_pitch(0, 0, pitch)).unwrap();
        }
        assert_eq!(pitches_of(&score, 0, 0), vec![60, 64, 67]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_pitch_written_on_a_guitar_goes_on_a_free_string_at_the_lowest_fret() {
        let mut score = blank_score("t");
        // A3 is fret 2 of string 3; once that string is taken, fret 7 of string 4.
        apply_edit(&mut score, &add_pitch(0, 0, 57)).unwrap();
        apply_edit(&mut score, &set_fret(0, 0, 3, 0)).unwrap();
        apply_edit(&mut score, &add_pitch(0, 0, 57)).unwrap();
        let placed: Vec<Place> = beats(&score, 0)[0].notes.iter().map(|n| n.place).collect();
        assert_eq!(
            placed,
            vec![
                Place::Fretted { string: 3, fret: 0 },
                Place::Fretted { string: 4, fret: 7 }
            ]
        );
    }

    #[test]
    fn a_pitch_no_free_string_can_play_is_refused() {
        let mut score = blank_score("t");
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &add_pitch(0, 0, 20)),
            Err(EditError::NoFreeString)
        );
        assert_eq!(
            apply_edit(&mut score, &add_pitch(0, 0, 128)),
            Err(EditError::InvalidPitch)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn removing_a_pitch_leaves_the_rest_of_the_chord() {
        let mut score = piano_score();
        for pitch in [60, 64] {
            apply_edit(&mut score, &add_pitch(0, 0, pitch)).unwrap();
        }
        let remove = |pitch| Edit::RemovePitch {
            track: 0,
            measure: 0,
            beat: 0,
            pitch,
        };
        apply_edit(&mut score, &remove(60)).unwrap();
        assert_eq!(pitches_of(&score, 0, 0), vec![64]);
        apply_edit(&mut score, &remove(64)).unwrap();
        assert!(beats(&score, 0)[0].notes.is_empty(), "the beat is a rest");
    }

    #[test]
    fn moving_a_pitch_keeps_the_chord_sorted() {
        let mut score = piano_score();
        for pitch in [60, 64] {
            apply_edit(&mut score, &add_pitch(0, 0, pitch)).unwrap();
        }
        apply_edit(&mut score, &move_pitch(0, 0, 60, 66)).unwrap();
        assert_eq!(pitches_of(&score, 0, 0), vec![64, 66]);
    }

    #[test]
    fn moving_a_tied_pitch_unties_it_and_the_note_that_continued_it() {
        let mut score = piano_score();
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
        for beat in 0..3 {
            apply_edit(&mut score, &add_pitch(0, beat, 62)).unwrap();
        }
        for beat in 1..3 {
            let tie = Edit::SetPitchTied {
                track: 0,
                measure: 0,
                beat,
                pitch: 62,
                tied: true,
            };
            apply_edit(&mut score, &tie).unwrap();
        }
        apply_edit(&mut score, &move_pitch(0, 1, 62, 63)).unwrap();
        let ties: Vec<bool> = beats(&score, 0)[..3]
            .iter()
            .map(|b| b.notes[0].tied)
            .collect();
        assert_eq!(ties, vec![false, false, false]);
        assert_eq!(pitches_of(&score, 0, 1), vec![63]);
    }

    #[test]
    fn moving_onto_a_pitch_the_beat_has_or_from_one_it_lacks_is_refused() {
        let mut score = piano_score();
        for pitch in [60, 61] {
            apply_edit(&mut score, &add_pitch(0, 0, pitch)).unwrap();
        }
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &move_pitch(0, 0, 60, 61)),
            Err(EditError::PitchTaken)
        );
        assert_eq!(
            apply_edit(&mut score, &move_pitch(0, 0, 70, 71)),
            Err(EditError::NoSuchNote)
        );
        assert_eq!(
            apply_edit(&mut score, &move_pitch(0, 0, 61, 128)),
            Err(EditError::InvalidPitch)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn moving_a_guitar_note_keeps_its_string_when_it_reaches_the_pitch() {
        let mut score = blank_score("t");
        apply_edit(&mut score, &set_fret(0, 0, 1, 0)).unwrap();
        apply_edit(&mut score, &move_pitch(0, 0, 64, 65)).unwrap();
        assert_eq!(
            beats(&score, 0)[0].notes[0].place,
            Place::Fretted { string: 1, fret: 1 }
        );
        // Below the open first string: another string takes it.
        apply_edit(&mut score, &move_pitch(0, 0, 65, 63)).unwrap();
        assert_eq!(
            beats(&score, 0)[0].notes[0].place,
            Place::Fretted { string: 2, fret: 4 }
        );
    }

    #[test]
    fn a_pitched_tie_needs_the_same_pitch_just_before() {
        let mut score = piano_score();
        two_halves(&mut score, &[60], &[60, 64]);
        let tie = |pitch| Edit::SetPitchTied {
            track: 0,
            measure: 0,
            beat: 1,
            pitch,
            tied: true,
        };
        assert_eq!(
            apply_edit(&mut score, &tie(64)),
            Err(EditError::NothingToTie)
        );
        apply_edit(&mut score, &tie(60)).unwrap();
        let tied: Vec<bool> = beats(&score, 0)[1].notes.iter().map(|n| n.tied).collect();
        assert_eq!(tied, vec![true, false]);
    }

    #[test]
    fn strings_frets_and_tunings_are_refused_on_a_piano() {
        let mut score = piano_score();
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &set_fret(0, 0, 1, 3)),
            Err(EditError::NoStrings)
        );
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetTuning {
                    track: 0,
                    tuning: GUITAR_STANDARD.to_vec()
                }
            ),
            Err(EditError::NoStrings)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn clearing_by_pitch_keeps_the_notes_outside_the_span() {
        let mut score = piano_score();
        for pitch in [48, 60, 64, 72] {
            apply_edit(&mut score, &add_pitch(0, 0, pitch)).unwrap();
        }
        let edit = Edit::ClearNotes {
            track: 0,
            from: at(0, 0),
            to: at(0, 0),
            strings: None,
            pitches: Some(PitchSpan { low: 60, high: 64 }),
        };
        apply_edit(&mut score, &edit).unwrap();
        assert_eq!(pitches_of(&score, 0, 0), vec![48, 72]);
    }

    #[test]
    fn a_paste_keeps_only_the_notes_of_the_kind_of_the_track() {
        let mut score = piano_score();
        let beat = Beat {
            duration: dur(NoteValue::Quarter, 0),
            notes: vec![Note::fretted(1, 0), Note::pitched(67)],
        };
        apply_edit(&mut score, &paste_edit(0, 0, 0, vec![beat])).unwrap();
        assert_eq!(beats(&score, 0)[0].notes, vec![Note::pitched(67)]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_guitar_becomes_a_piano_with_the_same_pitches_and_ties() {
        let mut score = demo_score();
        let guitar = score.tracks[0].clone();
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Piano,
            },
        )
        .unwrap();
        let piano = &score.tracks[0];
        assert!(piano.tuning.is_empty());
        assert_eq!(piano.sound, Sound::Piano);
        for (g, p) in guitar.measures.iter().zip(&piano.measures) {
            for (a, b) in g.voices[0].beats.iter().zip(&p.voices[0].beats) {
                let mut pitches: Vec<u8> = a.notes.iter().filter_map(|n| guitar.pitch(n)).collect();
                pitches.sort_unstable();
                pitches.dedup();
                assert_eq!(b.notes.iter().map(Note::key).collect::<Vec<_>>(), pitches);
                assert_eq!(
                    a.notes.iter().any(|n| n.tied),
                    b.notes.iter().any(|n| n.tied)
                );
            }
        }
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_piano_becomes_a_guitar_when_its_pitches_fit_the_strings() {
        let mut score = piano_score();
        two_halves(&mut score, &[45, 52, 57], &[64]);
        let to_guitar = Edit::SetInstrument {
            track: 0,
            instrument: Instrument::Guitar,
        };
        apply_edit(&mut score, &to_guitar).unwrap();
        assert_eq!(score.tracks[0].tuning, GUITAR_STANDARD.to_vec());
        assert_eq!(pitches_of(&score, 0, 0), vec![57, 52, 45]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_piano_with_a_pitch_the_guitar_cannot_play_stays_a_piano() {
        let mut score = piano_score();
        apply_edit(&mut score, &add_pitch(0, 0, 21)).unwrap();
        let before = score.clone();
        let to_guitar = Edit::SetInstrument {
            track: 0,
            instrument: Instrument::Guitar,
        };
        assert_eq!(
            apply_edit(&mut score, &to_guitar),
            Err(EditError::NoFreeString)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_piano_track_is_added_without_strings() {
        let mut score = blank_score("t");
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Piano,
                sound: None,
            },
        )
        .unwrap();
        let piano = &score.tracks[1];
        assert_eq!((piano.name.as_str(), piano.sound), ("Piano", Sound::Piano));
        assert!(piano.tuning.is_empty());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn pitched_edits_travel_as_camel_case_json() {
        let json = serde_json::to_value(move_pitch(1, 2, 60, 61)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "movePitch", "track": 0, "measure": 1, "beat": 2, "pitch": 60, "to": 61 })
        );
        let clear: Edit = serde_json::from_str(
            r#"{"type":"clearNotes","track":0,"from":{"measure":0,"beat":0},"to":{"measure":0,"beat":0},"strings":null}"#,
        )
        .unwrap();
        assert!(matches!(clear, Edit::ClearNotes { pitches: None, .. }));
    }

    /// A blank score whose track is a drum kit: four measures of whole rests.
    fn drum_score() -> Score {
        let mut score = blank_score("d");
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Drums,
            },
        )
        .unwrap();
        score
    }

    fn keys_of(score: &Score, measure: usize, beat: usize) -> Vec<u8> {
        beats(score, measure)[beat]
            .notes
            .iter()
            .map(Note::key)
            .collect()
    }

    #[test]
    fn an_empty_track_becomes_a_drum_kit_without_strings() {
        let score = drum_score();
        let drums = &score.tracks[0];
        assert_eq!(drums.sound, Sound::DrumsStandard);
        assert!(drums.tuning.is_empty());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn pieces_added_to_a_beat_are_hit_together_once_each() {
        let mut score = drum_score();
        for piece in [42, 36, 42, 38] {
            apply_edit(&mut score, &add_pitch(0, 0, piece)).unwrap();
        }
        assert_eq!(keys_of(&score, 0, 0), vec![36, 38, 42]);
        assert_eq!(beats(&score, 0)[0].notes[0], Note::percussion(36));
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_key_that_is_no_drum_piece_is_refused() {
        let mut score = drum_score();
        let before = score.clone();
        for key in [34, 82, 127] {
            assert_eq!(
                apply_edit(&mut score, &add_pitch(0, 0, key)),
                Err(EditError::InvalidPiece)
            );
        }
        apply_edit(&mut score, &add_pitch(0, 0, 38)).unwrap();
        let with_snare = score.clone();
        assert_eq!(
            apply_edit(&mut score, &move_pitch(0, 0, 38, 90)),
            Err(EditError::InvalidPiece)
        );
        assert_eq!(score, with_snare);
        assert_ne!(score, before);
    }

    #[test]
    fn a_piece_moves_to_another_and_is_removed_by_its_key() {
        let mut score = drum_score();
        two_halves(&mut score, &[36, 42], &[38]);
        apply_edit(&mut score, &move_pitch(0, 0, 42, 46)).unwrap();
        assert_eq!(keys_of(&score, 0, 0), vec![36, 46]);
        assert_eq!(
            apply_edit(&mut score, &move_pitch(0, 0, 46, 36)),
            Err(EditError::PitchTaken)
        );
        apply_edit(
            &mut score,
            &Edit::RemovePitch {
                track: 0,
                measure: 0,
                beat: 0,
                pitch: 36,
            },
        )
        .unwrap();
        assert_eq!(keys_of(&score, 0, 0), vec![46]);
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn strings_and_tunings_are_refused_on_a_drum_track() {
        let mut score = drum_score();
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &set_fret(0, 0, 1, 3)),
            Err(EditError::NoStrings)
        );
        assert_eq!(
            apply_edit(
                &mut score,
                &Edit::SetTuning {
                    track: 0,
                    tuning: GUITAR_STANDARD.to_vec()
                }
            ),
            Err(EditError::NoStrings)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_paste_on_drums_keeps_only_drum_notes_without_technique() {
        let mut score = drum_score();
        let beat = Beat {
            duration: dur(NoteValue::Quarter, 0),
            notes: vec![
                Note::fretted(1, 0),
                Note::pitched(40),
                Note {
                    technique: Some(Technique::Bend),
                    ..Note::percussion(38)
                },
            ],
        };
        apply_edit(&mut score, &paste_edit(0, 0, 0, vec![beat])).unwrap();
        assert_eq!(beats(&score, 0)[0].notes, vec![Note::percussion(38)]);
        assert_eq!(score.validate(), Ok(()));

        let bad = Beat {
            duration: dur(NoteValue::Quarter, 0),
            notes: vec![Note::percussion(20)],
        };
        let before = score.clone();
        assert_eq!(
            apply_edit(&mut score, &paste_edit(0, 0, 0, vec![bad])),
            Err(EditError::InvalidPiece)
        );
        assert_eq!(score, before);
    }

    #[test]
    fn a_track_with_notes_does_not_change_to_or_from_drums() {
        let mut score = demo_score();
        let before = score.clone();
        let to_drums = Edit::SetInstrument {
            track: 0,
            instrument: Instrument::Drums,
        };
        assert_eq!(
            apply_edit(&mut score, &to_drums),
            Err(EditError::NotesOfAnotherKind)
        );
        assert_eq!(score, before);

        let mut drums = drum_score();
        apply_edit(&mut drums, &add_pitch(0, 0, 36)).unwrap();
        let with_kick = drums.clone();
        let to_piano = Edit::SetInstrument {
            track: 0,
            instrument: Instrument::Piano,
        };
        assert_eq!(
            apply_edit(&mut drums, &to_piano),
            Err(EditError::NotesOfAnotherKind)
        );
        assert_eq!(drums, with_kick);
    }

    #[test]
    fn an_empty_drum_track_becomes_a_guitar_with_its_strings() {
        let mut score = drum_score();
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Guitar,
            },
        )
        .unwrap();
        assert_eq!(score.tracks[0].tuning, GUITAR_STANDARD.to_vec());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn a_drum_hit_can_be_tied_to_the_same_piece_and_cleared_by_key_span() {
        let mut score = drum_score();
        two_halves(&mut score, &[36, 49], &[49]);
        apply_edit(
            &mut score,
            &Edit::SetPitchTied {
                track: 0,
                measure: 0,
                beat: 1,
                pitch: 49,
                tied: true,
            },
        )
        .unwrap();
        assert!(beats(&score, 0)[1].notes[0].tied);
        apply_edit(
            &mut score,
            &Edit::ClearNotes {
                track: 0,
                from: at(0, 0),
                to: at(0, 0),
                strings: None,
                pitches: Some(PitchSpan { low: 40, high: 60 }),
            },
        )
        .unwrap();
        assert_eq!(keys_of(&score, 0, 0), vec![36]);
        assert_eq!(score.validate(), Ok(()));
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

        let mut with_piano = demo_score();
        apply_edit(
            &mut with_piano,
            &Edit::SetInstrument {
                track: 1,
                instrument: Instrument::Piano,
            },
        )
        .unwrap();

        let mut with_drums = demo_score();
        apply_edit(
            &mut with_drums,
            &Edit::AddTrack {
                instrument: Instrument::Drums,
                sound: None,
            },
        )
        .unwrap();

        for start in [demo_score(), blank_score("t"), with_piano, with_drums] {
            let mut score = start;
            let mut applied = 0;
            for step in 0..600 {
                // Mostly valid addresses, so that most edits are accepted; sometimes wild ones.
                let track = next(score.tracks.len());
                let measure = next(score.tracks[track].measures.len());
                let beat = next(score.tracks[track].measures[measure].voices[0].beats.len());
                let string = 1 + next(score.tracks[track].tuning.len().max(1)) as u8;
                let wild = next(12) == 0;
                let pitch = if wild {
                    next(140) as u8
                } else {
                    30 + next(60) as u8
                };
                let (measure, beat) = if wild {
                    (next(20), next(20))
                } else {
                    (measure, beat)
                };

                let edit = match next(23) {
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
                        count: 1 + next(if wild { 3 } else { 1 }),
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
                        instrument: Instrument::ALL[next(Instrument::ALL.len())],
                        sound: [None, Some(Sound::ALL[next(Sound::ALL.len())])][next(2)],
                    },
                    10 => Edit::RemoveTrack {
                        index: next(score.tracks.len() + 1),
                    },
                    11 => Edit::SetTimeSignature {
                        measure,
                        numerator: next(8) as u8,
                        denominator: [1, 2, 4, 8, 16, 32, 3][next(7)],
                    },
                    16 => Edit::ClearNotes {
                        track,
                        from: BeatAddress { measure, beat },
                        to: BeatAddress {
                            measure: measure + next(2),
                            beat: next(4),
                        },
                        strings: [
                            None,
                            Some(StringSpan {
                                first: 1,
                                last: string,
                            }),
                        ][next(2)],
                        pitches: [
                            None,
                            Some(PitchSpan {
                                low: pitch,
                                high: pitch.saturating_add(next(12) as u8),
                            }),
                        ][next(2)],
                    },
                    18 | 19 => Edit::AddPitch {
                        track,
                        measure,
                        beat,
                        pitch,
                    },
                    20 => Edit::RemovePitch {
                        track,
                        measure,
                        beat,
                        pitch,
                    },
                    21 => Edit::MovePitch {
                        track,
                        measure,
                        beat,
                        pitch,
                        to: pitch.saturating_add(next(3) as u8).saturating_sub(1),
                    },
                    22 => Edit::SetPitchTied {
                        track,
                        measure,
                        beat,
                        pitch,
                        tied: next(3) != 0,
                    },
                    17 => Edit::RemoveRests {
                        track,
                        from: BeatAddress { measure, beat },
                        to: BeatAddress {
                            measure: measure + next(2),
                            beat: next(4),
                        },
                    },
                    12 => Edit::RenameTrack {
                        track,
                        name: ["Lead", "", "  Bass  "][next(3)].to_string(),
                    },
                    13 if next(2) == 0 => Edit::SetSound {
                        track,
                        sound: Sound::ALL[next(Sound::ALL.len())],
                    },
                    13 => Edit::SetInstrument {
                        track,
                        instrument: Instrument::ALL[next(Instrument::ALL.len())],
                    },
                    14 => Edit::Paste {
                        track,
                        measure,
                        beat,
                        beats: (0..1 + next(4))
                            .map(|_| Beat {
                                duration: dur(values[next(6)], next(3) as u8),
                                notes: match next(5) {
                                    0 => vec![],
                                    1 => vec![Note {
                                        tied: next(4) == 0,
                                        ..Note::pitched(pitch)
                                    }],
                                    2 => vec![Note {
                                        tied: next(4) == 0,
                                        technique: [None, Some(Technique::Slide)][next(2)],
                                        ..Note::percussion(pitch)
                                    }],
                                    _ => vec![Note {
                                        tied: next(4) == 0,
                                        ..Note::fretted(string, next(25) as u8)
                                    }],
                                },
                            })
                            .collect(),
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
