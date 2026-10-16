//! Export a `Score` as a Standard MIDI File (format 1): a first track with the title, tempo and
//! time signatures, then one track per score track on its own channel, with the General MIDI
//! program of its sound. A drum track plays on the percussion channel (10), its program choosing
//! the kit, its notes on the keys of their pieces. Ties become one long note. Techniques are not
//! written: MIDI has no standard way to say them.

use std::collections::HashMap;
use std::fmt;

use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
use score_model::{Score, Track};

/// Pulses per quarter note. A tick of the score (1/128 of a whole note) is 15 of them.
const PPQ: u16 = 480;
const PULSES_PER_TICK: u32 = PPQ as u32 / 32;
const VELOCITY: u8 = 90;
const PERCUSSION_CHANNEL: u8 = 9;

#[derive(Debug, PartialEq, Eq)]
pub enum MidiExportError {
    /// The piece is longer than a MIDI file can express.
    TooLong,
    Write(String),
}

impl fmt::Display for MidiExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(f, "this piece is too long for a MIDI file"),
            Self::Write(message) => write!(f, "cannot write the MIDI file: {message}"),
        }
    }
}

impl std::error::Error for MidiExportError {}

pub fn export_midi(score: &Score) -> Result<Vec<u8>, MidiExportError> {
    let mut tracks: Vec<Vec<TrackEvent<'_>>> = Vec::with_capacity(score.tracks.len() + 1);
    tracks.push(conductor_track(score)?);
    for (index, track) in score.tracks.iter().enumerate() {
        tracks.push(instrument_track(track, index)?);
    }

    let smf = Smf {
        header: Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ))),
        tracks,
    };
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)
        .map_err(|e| MidiExportError::Write(e.to_string()))?;
    Ok(bytes)
}

/// Absolute position in pulses, then turned into the deltas a file stores.
fn with_deltas(
    mut events: Vec<(u32, TrackEventKind<'_>)>,
) -> Result<Vec<TrackEvent<'_>>, MidiExportError> {
    // Stable: at one position the order they were pushed in is kept.
    events.sort_by_key(|(at, _)| *at);
    let mut out = Vec::with_capacity(events.len() + 1);
    let mut previous = 0;
    for (at, kind) in events {
        let delta = u28::try_from(at - previous).ok_or(MidiExportError::TooLong)?;
        out.push(TrackEvent { delta, kind });
        previous = at;
    }
    out.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    Ok(out)
}

fn conductor_track(score: &Score) -> Result<Vec<TrackEvent<'_>>, MidiExportError> {
    let mut events: Vec<(u32, TrackEventKind<'_>)> = vec![
        (
            0,
            TrackEventKind::Meta(MetaMessage::TrackName(score.title.as_bytes())),
        ),
        (
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(
                60_000_000 / u32::from(score.tempo_bpm.max(1)),
            ))),
        ),
    ];

    // The time signature, written where it changes.
    let mut at = 0u32;
    let mut previous = None;
    for measure in score.tracks.first().map_or(&[][..], |t| &t.measures[..]) {
        let signature = measure.time_signature;
        if previous != Some(signature) {
            events.push((
                at,
                TrackEventKind::Meta(MetaMessage::TimeSignature(
                    signature.numerator,
                    signature.denominator.trailing_zeros() as u8,
                    24,
                    8,
                )),
            ));
            previous = Some(signature);
        }
        at = at
            .checked_add(signature.ticks() * PULSES_PER_TICK)
            .ok_or(MidiExportError::TooLong)?;
    }
    with_deltas(events)
}

fn instrument_track<'a>(
    track: &'a Track,
    index: usize,
) -> Result<Vec<TrackEvent<'a>>, MidiExportError> {
    // One channel per track, skipping the percussion channel, like the player does; every drum
    // track plays on the percussion channel.
    let channel = (index % 15) as u8;
    let channel = u4::new(if track.sound.is_drum_kit() {
        PERCUSSION_CHANNEL
    } else if channel >= PERCUSSION_CHANNEL {
        channel + 1
    } else {
        channel
    });
    let program = track.midi_program();

    let mut events: Vec<(u32, TrackEventKind<'_>)> = vec![
        (
            0,
            TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes())),
        ),
        (
            0,
            TrackEventKind::Midi {
                channel,
                message: MidiMessage::ProgramChange {
                    program: u7::new(program),
                },
            },
        ),
    ];

    let voices = track
        .measures
        .iter()
        .map(|m| m.voices.len())
        .max()
        .unwrap_or(0);
    for voice in 0..voices {
        // (start, end, pitch) in pulses; a tie pushes back the end of the note it continues.
        let mut notes: Vec<(u32, u32, u8)> = Vec::new();
        let mut sounding: HashMap<u8, usize> = HashMap::new();
        let mut measure_start = 0u32;

        for measure in &track.measures {
            let mut at = measure_start;
            for beat in measure.voices.get(voice).map_or(&[][..], |v| &v.beats[..]) {
                let end = at + beat.duration.ticks() * PULSES_PER_TICK;
                let mut next = HashMap::new();
                for note in &beat.notes {
                    let Some(pitch) = track.pitch(note) else {
                        continue;
                    };
                    let index = match note.tied.then(|| sounding.get(&pitch)).flatten() {
                        Some(&held) => {
                            notes[held].1 = end;
                            held
                        }
                        None => {
                            notes.push((at, end, pitch));
                            notes.len() - 1
                        }
                    };
                    next.insert(pitch, index);
                }
                sounding = next;
                at = end;
            }
            measure_start += measure.time_signature.ticks() * PULSES_PER_TICK;
        }

        for (start, end, pitch) in notes {
            let key = u7::new(pitch.min(127));
            events.push((
                start,
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOn {
                        key,
                        vel: u7::new(VELOCITY),
                    },
                },
            ));
            events.push((
                end,
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOff {
                        key,
                        vel: u7::new(0),
                    },
                },
            ));
        }
    }

    // At one position, note-offs go first so a repeated pitch is struck again.
    events.sort_by_key(|(at, kind)| {
        let is_on = matches!(
            kind,
            TrackEventKind::Midi {
                message: MidiMessage::NoteOn { .. },
                ..
            }
        );
        (*at, is_on)
    });
    with_deltas(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_midi;
    use score_model::{blank_score, demo_score, Edit, Instrument, Sound};

    /// (start, end, pitch) of every sounding note of a track, ties merged, sorted.
    fn sounding(track: &Track) -> Vec<(u32, u32, u8)> {
        let mut notes: Vec<(u32, u32, u8)> = Vec::new();
        let mut held: HashMap<u8, usize> = HashMap::new();
        let mut measure_start = 0;
        for measure in &track.measures {
            let mut at = measure_start;
            for beat in &measure.voices[0].beats {
                let end = at + beat.duration.ticks();
                let mut next = HashMap::new();
                for note in &beat.notes {
                    let pitch = track.pitch(note).unwrap();
                    let index = match note.tied.then(|| held.get(&pitch)).flatten() {
                        Some(&i) => {
                            notes[i].1 = end;
                            i
                        }
                        None => {
                            notes.push((at, end, pitch));
                            notes.len() - 1
                        }
                    };
                    next.insert(pitch, index);
                }
                held = next;
                at = end;
            }
            measure_start += measure.time_signature.ticks();
        }
        notes.sort();
        notes
    }

    #[test]
    fn the_file_has_a_conductor_track_and_one_track_per_instrument() {
        let bytes = export_midi(&demo_score()).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        assert_eq!(smf.header.format, Format::Parallel);
        assert_eq!(smf.tracks.len(), 3);
    }

    #[test]
    fn a_pack_sound_is_written_with_its_fallback_program() {
        let mut original = demo_score();
        let edit = Edit::SetSound {
            track: 0,
            sound: Sound::parse("guitar/twelve-string").unwrap(),
            fallback_program: Some(25),
        };
        score_model::apply_edit(&mut original, &edit).unwrap();

        let imported = import_midi(&export_midi(&original).unwrap()).unwrap().score;

        // A MIDI file only knows General MIDI: the track comes back as the steel guitar.
        assert_eq!(imported.tracks[0].sound, Sound::GUITAR_STEEL);
        assert_eq!(sounding(&imported.tracks[0]), sounding(&original.tracks[0]));
        assert_eq!(imported.validate(), Ok(()));
    }

    #[test]
    fn exporting_then_importing_gives_back_the_same_notes() {
        let original = demo_score();
        let imported = import_midi(&export_midi(&original).unwrap()).unwrap().score;

        assert_eq!(imported.tempo_bpm, original.tempo_bpm);
        assert_eq!(imported.tracks.len(), original.tracks.len());
        for (a, b) in original.tracks.iter().zip(&imported.tracks) {
            assert_eq!(a.instrument, b.instrument);
            assert_eq!(sounding(a), sounding(b), "track {}", a.name);
        }
        assert_eq!(imported.title, original.title);
    }

    #[test]
    fn every_sound_survives_a_round_trip() {
        let mut score = blank_score("t");
        for sound in Sound::ALL {
            score_model::apply_edit(
                &mut score,
                &Edit::AddTrack {
                    instrument: sound.instrument(),
                    sound: Some(sound),
                    fallback_program: None,
                },
            )
            .unwrap();
            let track = score.tracks.len() - 1;
            // A note on the last string, so that the banjo plays its short fifth string; a
            // chord on the piano.
            let string = score.tracks[track].tuning.len() as u8;
            let edits = if string == 0 {
                [48, 64, 67]
                    .map(|pitch| Edit::AddPitch {
                        track,
                        measure: 0,
                        beat: 0,
                        pitch,
                    })
                    .to_vec()
            } else {
                vec![Edit::SetFret {
                    track,
                    measure: 0,
                    beat: 0,
                    string,
                    fret: 7,
                }]
            };
            for edit in edits {
                score_model::apply_edit(&mut score, &edit).unwrap();
            }
        }
        // The blank first track has no notes, which the importer leaves out.
        score_model::apply_edit(&mut score, &Edit::RemoveTrack { index: 0 }).unwrap();
        let imported = import_midi(&export_midi(&score).unwrap()).unwrap().score;

        assert_eq!(imported.tracks.len(), score.tracks.len());
        for (a, b) in score.tracks.iter().zip(&imported.tracks) {
            assert_eq!(b.sound, a.sound);
            assert_eq!(b.tuning, a.tuning);
            assert_eq!(b.clef, a.clef, "{:?}", a.sound);
            assert_eq!(sounding(a), sounding(b), "{:?}", a.sound);
        }
    }

    #[test]
    fn a_tie_is_written_as_one_long_note() {
        let mut score = blank_score("t");
        score_model::apply_edit(
            &mut score,
            &Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 0,
                string: 1,
                fret: 3,
            },
        )
        .unwrap();
        score_model::apply_edit(
            &mut score,
            &Edit::SetFret {
                track: 0,
                measure: 1,
                beat: 0,
                string: 1,
                fret: 3,
            },
        )
        .unwrap();
        score_model::apply_edit(
            &mut score,
            &Edit::SetTied {
                track: 0,
                measure: 1,
                beat: 0,
                string: 1,
                tied: true,
            },
        )
        .unwrap();

        let bytes = export_midi(&score).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        let note_ons = smf.tracks[1]
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TrackEventKind::Midi {
                        message: MidiMessage::NoteOn { .. },
                        ..
                    }
                )
            })
            .count();
        assert_eq!(note_ons, 1);
    }

    #[test]
    fn the_time_signature_is_written_where_it_changes() {
        let mut score = blank_score("t");
        score_model::apply_edit(
            &mut score,
            &Edit::SetTimeSignature {
                measure: 2,
                numerator: 3,
                denominator: 4,
            },
        )
        .unwrap();
        let bytes = export_midi(&score).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        let signatures: Vec<(u8, u8)> = smf.tracks[0]
            .iter()
            .filter_map(|e| match e.kind {
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => Some((n, d)),
                _ => None,
            })
            .collect();
        // 4/4 for two measures, then 3/4 (the denominator is stored as a power of two).
        assert_eq!(signatures, vec![(4, 2), (3, 2)]);
    }

    #[test]
    fn every_drum_piece_survives_a_round_trip_on_the_percussion_channel() {
        let mut score = blank_score("t");
        score_model::apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Drums,
            },
        )
        .unwrap();
        score_model::apply_edit(
            &mut score,
            &Edit::SetSound {
                track: 0,
                sound: Sound::DRUMS_BRUSH,
                fallback_program: None,
            },
        )
        .unwrap();
        // One quarter note per piece, in as many measures as it takes.
        let pieces: Vec<u8> = score_model::DRUM_PIECES.collect();
        let measures = pieces.len().div_ceil(4);
        for at in score.tracks[0].measures.len()..measures {
            score_model::apply_edit(&mut score, &Edit::InsertMeasure { at }).unwrap();
        }
        for measure in 0..measures {
            for beat in 0..4 {
                let quarter = score_model::Duration {
                    value: score_model::NoteValue::Quarter,
                    dots: 0,
                };
                score_model::apply_edit(
                    &mut score,
                    &Edit::SetDuration {
                        track: 0,
                        measure,
                        beat,
                        duration: quarter,
                    },
                )
                .unwrap();
            }
        }
        for (i, &piece) in pieces.iter().enumerate() {
            score_model::apply_edit(
                &mut score,
                &Edit::AddPitch {
                    track: 0,
                    measure: i / 4,
                    beat: i % 4,
                    pitch: piece,
                },
            )
            .unwrap();
        }

        let bytes = export_midi(&score).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        for event in &smf.tracks[1] {
            if let TrackEventKind::Midi { channel, message } = event.kind {
                assert_eq!(channel.as_int(), PERCUSSION_CHANNEL);
                if let MidiMessage::ProgramChange { program } = message {
                    assert_eq!(program.as_int(), 40);
                }
            }
        }
        let imported = import_midi(&bytes).unwrap();
        assert_eq!(imported.skipped_notes, 0);
        let drums = &imported.score.tracks[0];
        assert_eq!(drums.instrument, Instrument::Drums);
        assert_eq!(drums.sound, Sound::DRUMS_BRUSH);
        let original = &score.tracks[0];
        for (a, b) in original.measures.iter().zip(&drums.measures) {
            assert_eq!(a.voices[0].beats, b.voices[0].beats);
        }
    }

    #[test]
    fn a_score_with_many_tracks_never_uses_the_percussion_channel() {
        let mut score = blank_score("t");
        for _ in 0..20 {
            score_model::apply_edit(
                &mut score,
                &Edit::AddTrack {
                    instrument: Instrument::Bass,
                    sound: None,
                    fallback_program: None,
                },
            )
            .unwrap();
        }
        let bytes = export_midi(&score).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        for track in &smf.tracks {
            for event in track {
                if let TrackEventKind::Midi { channel, .. } = event.kind {
                    assert_ne!(channel.as_int(), PERCUSSION_CHANNEL);
                }
            }
        }
    }
}
