//! Import MIDI -> `Score`.
//!
//! Each (MIDI track, channel) that plays notes becomes a track of the score, so a format 0 file
//! with several channels is split. The percussion channel (10) is not imported. The instrument
//! comes from the General MIDI program: bass for 32-39, violin for 40, guitar otherwise.
//!
//! A note whose duration cannot be written as a single value, or that crosses a barline, is
//! split into tied notes (`Note::tied`). Timing is quantized to a 32nd note, so triplets and
//! live-played files lose their feel.
//!
//! The importer never panics on odd input: it returns an error or leaves notes out and counts them.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use score_model::{
    decompose_ticks, place_chord, Beat, Duration, Instrument, Measure, Note, Score, TimeSignature,
    Track, Voice,
};

/// Smallest unit: 1/128 of a whole note. Notes are quantized to a 32nd note (4).
const GRID: u32 = 4;
const PERCUSSION_CHANNEL: u8 = 9;
/// Longest piece accepted. Protects against files that would allocate millions of measures.
const MAX_MEASURES: u32 = 5000;
/// Fastest tempo the score model accepts.
const MAX_TEMPO: u16 = 999;

#[derive(Debug)]
pub enum MidiImportError {
    Parse(midly::Error),
    UnsupportedTiming,
    TooLong,
    /// The imported score failed its own checks. This is a bug in the importer.
    Invalid(String),
}

impl fmt::Display for MidiImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "invalid MIDI file: {e}"),
            Self::UnsupportedTiming => write!(f, "this MIDI time division is not supported"),
            Self::TooLong => write!(
                f,
                "the piece is too long (more than {MAX_MEASURES} measures)"
            ),
            Self::Invalid(reason) => {
                write!(f, "the MIDI file could not be converted: {reason}")
            }
        }
    }
}

impl std::error::Error for MidiImportError {}

impl From<midly::Error> for MidiImportError {
    fn from(e: midly::Error) -> Self {
        Self::Parse(e)
    }
}

pub struct MidiImport {
    pub score: Score,
    /// Notes left out because no string of the instrument could play them: out of range, or more
    /// notes in a chord than the instrument has strings.
    pub skipped_notes: usize,
}

/// A note as read from the file: start and end in 1/128 of a whole note, and MIDI pitch.
type RawNote = (u32, u32, u8);

#[derive(Default)]
struct ChannelNotes {
    program: Option<u8>,
    notes: Vec<RawNote>,
}

pub fn import_midi(bytes: &[u8]) -> Result<MidiImport, MidiImportError> {
    let smf = Smf::parse(bytes)?;
    let ppq = match smf.header.timing {
        Timing::Metrical(t) if t.as_int() > 0 => u32::from(t.as_int()),
        _ => return Err(MidiImportError::UnsupportedTiming),
    };

    let mut tempo_bpm = 120;
    let mut time_signature = None;
    let mut tempo_seen = false;
    let mut title = None;
    // (name of the MIDI track, channels that play notes in it)
    let mut sources: Vec<(String, BTreeMap<u8, ChannelNotes>)> = Vec::new();

    for (track_index, midi_track) in smf.tracks.iter().enumerate() {
        let mut abs: u64 = 0;
        let mut open: HashMap<(u8, u8), u64> = HashMap::new();
        let mut channels: BTreeMap<u8, ChannelNotes> = BTreeMap::new();
        let mut name = None;

        for ev in midi_track {
            abs += u64::from(ev.delta.as_int());
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) if !tempo_seen => {
                    let bpm = 60_000_000 / us.as_int().max(1);
                    tempo_bpm = u16::try_from(bpm).unwrap_or(MAX_TEMPO).clamp(1, MAX_TEMPO);
                    tempo_seen = true;
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _))
                    if time_signature.is_none() =>
                {
                    time_signature = usable_signature(n, d);
                }
                TrackEventKind::Meta(MetaMessage::TrackName(n)) if name.is_none() => {
                    name = Some(String::from_utf8_lossy(n).trim().to_string());
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        MidiMessage::ProgramChange { program } => {
                            channels
                                .entry(ch)
                                .or_default()
                                .program
                                .get_or_insert(program.as_int());
                        }
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            // A second note-on for a held key ends the first one.
                            if let Some(start) = open.insert((ch, key.as_int()), abs) {
                                push_note(&mut channels, ch, key.as_int(), start, abs, ppq);
                            }
                        }
                        MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                            if let Some(start) = open.remove(&(ch, key.as_int())) {
                                push_note(&mut channels, ch, key.as_int(), start, abs, ppq);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Notes never released last until the end of the track.
        for ((ch, key), start) in open {
            push_note(&mut channels, ch, key, start, abs, ppq);
        }

        if track_index == 0 {
            title = name.clone().filter(|n| !n.is_empty());
        }
        channels.retain(|&ch, c| ch != PERCUSSION_CHANNEL && !c.notes.is_empty());
        if !channels.is_empty() {
            sources.push((
                name.filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("Track {}", track_index + 1)),
                channels,
            ));
        }
    }

    let time_signature = time_signature.unwrap_or(TimeSignature {
        numerator: 4,
        denominator: 4,
    });
    let mut tracks = Vec::new();
    let mut skipped_notes = 0;
    for (name, channels) in sources {
        let is_split = channels.len() > 1;
        for (ch, channel) in channels {
            let name = if is_split {
                format!("{name} (ch {})", ch + 1)
            } else {
                name.clone()
            };
            let (track, skipped) = build_track(name, &channel, time_signature)?;
            skipped_notes += skipped;
            tracks.push(track);
        }
    }

    pad_to_same_length(&mut tracks, time_signature);
    let score = Score {
        title: title.unwrap_or_else(|| "Untitled".into()),
        tempo_bpm,
        tracks,
    };
    score
        .validate()
        .map_err(|e| MidiImportError::Invalid(e.to_string()))?;

    Ok(MidiImport {
        score,
        skipped_notes,
    })
}

/// Every track ends up with the same number of measures, so that measures can be inserted and
/// deleted for the whole score at once.
fn pad_to_same_length(tracks: &mut [Track], ts: TimeSignature) {
    let longest = tracks.iter().map(|t| t.measures.len()).max().unwrap_or(0);
    for track in tracks {
        while track.measures.len() < longest {
            track.measures.push(Measure {
                time_signature: ts,
                voices: vec![Voice {
                    beats: decompose(ts.ticks())
                        .into_iter()
                        .map(|duration| Beat {
                            duration,
                            notes: vec![],
                        })
                        .collect(),
                }],
            });
        }
    }
}

/// A time signature the grid can represent: whole multiples of a 32nd note per measure.
fn usable_signature(numerator: u8, denominator_power: u8) -> Option<TimeSignature> {
    let denominator = 1u32.checked_shl(u32::from(denominator_power))?;
    let signature = TimeSignature {
        numerator,
        denominator: u8::try_from(denominator).ok()?,
    };
    let ticks = u32::from(numerator) * 128 / denominator;
    (numerator > 0 && denominator <= 32 && ticks >= GRID && ticks.is_multiple_of(GRID))
        .then_some(signature)
}

fn push_note(
    channels: &mut BTreeMap<u8, ChannelNotes>,
    channel: u8,
    key: u8,
    start: u64,
    end: u64,
    ppq: u32,
) {
    let s = quantize(start, ppq);
    let e = quantize(end, ppq).max(s.saturating_add(GRID));
    channels.entry(channel).or_default().notes.push((s, e, key));
}

/// MIDI ticks to 1/128-whole-note units, rounded to the grid. Saturates on absurd values.
fn quantize(tick: u64, ppq: u32) -> u32 {
    let units = (tick.saturating_mul(32) + u64::from(ppq) / 2) / u64::from(ppq);
    let units = u32::try_from(units).unwrap_or(u32::MAX / 2);
    (units + GRID / 2) / GRID * GRID
}

fn instrument_for(program: Option<u8>) -> Instrument {
    match program {
        Some(32..=39) => Instrument::Bass,
        Some(40) => Instrument::Violin,
        _ => Instrument::Guitar,
    }
}

/// Splits a length into durations. `len` is a multiple of `GRID`, so this always succeeds.
fn decompose(len: u32) -> Vec<Duration> {
    decompose_ticks(len).expect("len is a multiple of GRID")
}

fn build_track(
    name: String,
    channel: &ChannelNotes,
    ts: TimeSignature,
) -> Result<(Track, usize), MidiImportError> {
    let instrument = instrument_for(channel.program);
    let tuning = instrument.standard_tuning().to_vec();
    let mut skipped_notes = 0;

    // Group by start time: chords.
    let mut by_start: BTreeMap<u32, Vec<(u32, u8)>> = BTreeMap::new();
    for &(s, e, p) in &channel.notes {
        by_start.entry(s).or_default().push((e, p));
    }

    // Segments (start, length, notes) with rests in the gaps.
    let starts: Vec<u32> = by_start.keys().copied().collect();
    let mut segments: Vec<(u32, u32, Vec<Note>)> = Vec::new();
    let mut cursor = 0;
    for (i, &start) in starts.iter().enumerate() {
        if start > cursor {
            segments.push((cursor, start - cursor, vec![]));
        }
        let group = &by_start[&start];
        let max_end = group.iter().map(|&(e, _)| e).max().unwrap_or(start + GRID);
        let end = starts.get(i + 1).map_or(max_end, |&next| max_end.min(next));
        let pitches: Vec<u8> = group.iter().map(|&(_, p)| p).collect();
        let (chord, skipped) = place_chord(&tuning, &pitches, &[]);
        skipped_notes += skipped;
        segments.push((start, end - start, chord));
        cursor = end;
        if let Some(&next) = starts.get(i + 1) {
            if next > cursor {
                segments.push((cursor, next - cursor, vec![]));
                cursor = next;
            }
        }
    }

    // Split into measures.
    let measure_len = ts.ticks();
    let total = segments.last().map_or(0, |&(s, l, _)| s + l);
    let measure_count = total.div_ceil(measure_len).max(1);
    if measure_count > MAX_MEASURES {
        return Err(MidiImportError::TooLong);
    }
    let mut measures: Vec<Vec<Beat>> = vec![Vec::new(); measure_count as usize];

    for (start, len, chord) in segments {
        let mut pos = start;
        let end = start + len;
        let mut is_continuation = false;
        while pos < end {
            let m = (pos / measure_len) as usize;
            let piece_end = end.min((m as u32 + 1) * measure_len);
            for duration in decompose(piece_end - pos) {
                let notes = chord
                    .iter()
                    .map(|n| Note {
                        tied: is_continuation,
                        ..n.clone()
                    })
                    .collect();
                measures[m].push(Beat { duration, notes });
                is_continuation = true;
            }
            pos = piece_end;
        }
    }

    // Pad the last measure with a rest.
    let last_len: u32 = measures
        .last()
        .map_or(0, |b| b.iter().map(|b| b.duration.ticks()).sum());
    if last_len < measure_len {
        for duration in decompose(measure_len - last_len) {
            measures
                .last_mut()
                .expect("at least one measure")
                .push(Beat {
                    duration,
                    notes: vec![],
                });
        }
    }

    let track = Track {
        name,
        instrument,
        tuning,
        measures: measures
            .into_iter()
            .map(|beats| Measure {
                time_signature: ts,
                voices: vec![Voice { beats }],
            })
            .collect(),
    };
    Ok((track, skipped_notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{num::u4, Format, Header, MetaMessage, TrackEvent};

    const PPQ: u16 = 480;

    fn import(bytes: &[u8]) -> Result<Score, MidiImportError> {
        import_midi(bytes).map(|i| i.score)
    }

    fn note(delta: u32, key: u8, on: bool) -> TrackEvent<'static> {
        let message = if on {
            MidiMessage::NoteOn {
                key: key.into(),
                vel: 100.into(),
            }
        } else {
            MidiMessage::NoteOff {
                key: key.into(),
                vel: 0.into(),
            }
        };
        TrackEvent {
            delta: delta.into(),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message,
            },
        }
    }

    fn smf_bytes(events: Vec<TrackEvent<'static>>) -> Vec<u8> {
        let mut track = vec![
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(500_000.into())),
            },
            TrackEvent {
                delta: 0.into(),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"Lead")),
            },
        ];
        track.extend(events);
        track.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(PPQ.into())),
            tracks: vec![track],
        };
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }

    fn quarters(keys: &[u8]) -> Vec<TrackEvent<'static>> {
        keys.iter()
            .flat_map(|&k| [note(0, k, true), note(480, k, false)])
            .collect()
    }

    #[test]
    fn four_quarters_make_one_complete_measure() {
        let score = import(&smf_bytes(quarters(&[64, 59, 55, 40]))).unwrap();
        assert_eq!(score.tempo_bpm, 120);
        assert_eq!(score.title, "Lead");
        let track = &score.tracks[0];
        assert_eq!(track.measures.len(), 1);
        assert!(track.measures[0].is_complete());
        let beats = &track.measures[0].voices[0].beats;
        // 64 = open high E (string 1), 40 = open low E (string 6).
        assert_eq!(
            beats[0].notes,
            vec![Note {
                string: 1,
                fret: 0,
                technique: None,
                tied: false,
            }]
        );
        assert_eq!(
            beats[3].notes,
            vec![Note {
                string: 6,
                fret: 0,
                technique: None,
                tied: false,
            }]
        );
    }

    #[test]
    fn gap_becomes_rest_and_measures_split() {
        // 4 quarters, then a quarter rest, then a quarter: 2 measures.
        let mut ev = quarters(&[64, 64, 64, 64]);
        ev.extend([note(480, 64, true), note(480, 64, false)]);
        let score = import(&smf_bytes(ev)).unwrap();
        let measures = &score.tracks[0].measures;
        assert_eq!(measures.len(), 2);
        assert!(measures.iter().all(Measure::is_complete));
        assert!(measures[1].voices[0].beats[0].notes.is_empty());
        assert_eq!(measures[1].voices[0].beats[1].notes.len(), 1);
    }

    #[test]
    fn note_crossing_a_barline_is_split_into_tied_notes() {
        // 3 quarter rests, then a half note (2 quarters) straddling 2 measures.
        let ev = vec![note(1440, 64, true), note(960, 64, false)];
        let score = import(&smf_bytes(ev)).unwrap();
        let measures = &score.tracks[0].measures;
        assert_eq!(measures.len(), 2);
        assert!(measures.iter().all(Measure::is_complete));

        let last_of_first = measures[0].voices[0].beats.last().unwrap();
        let first_of_second = &measures[1].voices[0].beats[0];
        assert_eq!(last_of_first.notes.len(), 1);
        assert!(!last_of_first.notes[0].tied);
        assert_eq!(first_of_second.notes.len(), 1);
        assert!(first_of_second.notes[0].tied);
    }

    #[test]
    fn chord_keeps_all_notes() {
        let ev = vec![
            note(0, 64, true),
            note(0, 55, true),
            note(1920, 64, false),
            note(0, 55, false),
        ];
        let score = import(&smf_bytes(ev)).unwrap();
        let first = &score.tracks[0].measures[0].voices[0].beats[0];
        assert_eq!(first.notes.len(), 2);
    }

    #[test]
    fn invalid_data_is_an_error() {
        assert!(import_midi(b"not midi").is_err());
    }

    fn on_channel(channel: u8, delta: u32, key: u8, on: bool) -> TrackEvent<'static> {
        let event = note(delta, key, on);
        match event.kind {
            TrackEventKind::Midi { message, .. } => TrackEvent {
                delta: event.delta,
                kind: TrackEventKind::Midi {
                    channel: u4::new(channel),
                    message,
                },
            },
            _ => unreachable!(),
        }
    }

    fn program(channel: u8, program: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Midi {
                channel: u4::new(channel),
                message: MidiMessage::ProgramChange {
                    program: program.into(),
                },
            },
        }
    }

    fn write(ppq: u16, tracks: Vec<Vec<TrackEvent<'static>>>) -> Vec<u8> {
        let end = TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        };
        let tracks = tracks
            .into_iter()
            .map(|mut t| {
                t.push(end);
                t
            })
            .collect();
        let smf = Smf {
            header: Header::new(Format::Parallel, Timing::Metrical(ppq.into())),
            tracks,
        };
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }

    #[test]
    fn tracks_of_different_lengths_are_padded_to_the_same_number_of_measures() {
        let long = vec![program(0, 24)]
            .into_iter()
            .chain(quarters(&[64; 12]))
            .collect::<Vec<_>>();
        let short: Vec<_> = vec![program(1, 33)]
            .into_iter()
            .chain([on_channel(1, 0, 40, true), on_channel(1, 480, 40, false)])
            .collect();
        let score = import_midi(&write(PPQ, vec![long, short])).unwrap().score;

        assert_eq!(score.tracks[0].measures.len(), 3);
        assert_eq!(score.tracks[1].measures.len(), 3);
        assert!(score.tracks[1].measures[2].is_complete());
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn channels_of_one_track_become_separate_tracks() {
        let events = vec![
            on_channel(0, 0, 64, true),
            on_channel(1, 0, 40, true),
            on_channel(0, 480, 64, false),
            on_channel(1, 0, 40, false),
        ];
        let result = import_midi(&write(PPQ, vec![events])).unwrap();
        assert_eq!(result.score.tracks.len(), 2);
        assert!(result.score.tracks[0].name.ends_with("(ch 1)"));
        assert!(result.score.tracks[1].name.ends_with("(ch 2)"));
    }

    #[test]
    fn percussion_channel_is_not_imported() {
        let events = vec![on_channel(9, 0, 36, true), on_channel(9, 480, 36, false)];
        let result = import_midi(&write(PPQ, vec![events])).unwrap();
        assert!(result.score.tracks.is_empty());
        assert_eq!(result.skipped_notes, 0);
    }

    #[test]
    fn program_selects_the_instrument_and_its_tuning() {
        let bass = vec![
            program(0, 33),
            on_channel(0, 0, 40, true),
            on_channel(0, 480, 40, false),
        ];
        let violin = vec![
            program(0, 40),
            on_channel(0, 0, 76, true),
            on_channel(0, 480, 76, false),
        ];
        let result = import_midi(&write(PPQ, vec![bass, violin])).unwrap();

        let [bass, violin] = &result.score.tracks[..] else {
            panic!("two tracks expected")
        };
        assert_eq!(bass.instrument, Instrument::Bass);
        assert_eq!(bass.tuning.len(), 4);
        assert_eq!(violin.instrument, Instrument::Violin);
    }

    #[test]
    fn chord_notes_never_share_a_string() {
        // E4 and F4 would both pick the high E string; the second must move.
        let events = vec![
            note(0, 64, true),
            note(0, 65, true),
            note(480, 64, false),
            note(0, 65, false),
        ];
        let score = import(&write(PPQ, vec![events])).unwrap();
        let chord = &score.tracks[0].measures[0].voices[0].beats[0].notes;
        assert_eq!(chord.len(), 2);
        assert_ne!(chord[0].string, chord[1].string);
        for n in chord {
            assert!(score.tracks[0].pitch(n).is_some());
        }
    }

    #[test]
    fn notes_that_cannot_be_played_are_counted() {
        // A chord of 7 distinct pitches on 6 strings, plus one note below the lowest string.
        let pitches = [40, 45, 50, 55, 59, 64, 67];
        let mut events: Vec<_> = pitches.iter().map(|&k| note(0, k, true)).collect();
        events.extend(
            pitches
                .iter()
                .enumerate()
                .map(|(i, &k)| note(if i == 0 { 480 } else { 0 }, k, false)),
        );
        events.extend([note(0, 20, true), note(480, 20, false)]);

        let result = import_midi(&write(PPQ, vec![events])).unwrap();
        assert_eq!(result.skipped_notes, 2);
    }

    #[test]
    fn a_note_that_is_never_released_is_kept() {
        let events = vec![note(0, 64, true), note(960, 59, true)];
        let score = import(&write(PPQ, vec![events])).unwrap();
        let beats = &score.tracks[0].measures[0].voices[0].beats;
        assert!(beats.iter().any(|b| !b.notes.is_empty()));
        assert!(score.tracks[0].measures.iter().all(Measure::is_complete));
    }

    #[test]
    fn the_name_of_the_first_track_is_the_title() {
        let title = TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(b"My Song")),
        };
        let lead = vec![note(0, 64, true), note(480, 64, false)];
        let score = import(&write(PPQ, vec![vec![title], lead])).unwrap();
        assert_eq!(score.title, "My Song");
    }

    #[test]
    fn a_zero_time_division_is_an_error_not_a_crash() {
        let events = vec![note(0, 64, true), note(480, 64, false)];
        assert!(matches!(
            import_midi(&write(0, vec![events])),
            Err(MidiImportError::UnsupportedTiming)
        ));
    }

    #[test]
    fn a_time_signature_the_grid_cannot_hold_falls_back_to_four_four() {
        // 3/64 is 6 units per measure, which is not a multiple of a 32nd note.
        let signature = TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(3, 6, 24, 8)),
        };
        let mut events = vec![signature];
        events.extend(quarters(&[64]));
        let score = import(&write(PPQ, vec![events])).unwrap();
        let ts = score.tracks[0].measures[0].time_signature;
        assert_eq!((ts.numerator, ts.denominator), (4, 4));
    }

    #[test]
    fn a_waltz_keeps_its_time_signature() {
        let signature = TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(3, 2, 24, 8)),
        };
        let mut events = vec![signature];
        events.extend(quarters(&[64, 64, 64]));
        let score = import(&write(PPQ, vec![events])).unwrap();
        let measure = &score.tracks[0].measures[0];
        assert_eq!(measure.time_signature.numerator, 3);
        assert!(measure.is_complete());
    }

    #[test]
    fn an_enormous_gap_is_refused_instead_of_allocating_the_world() {
        let events = vec![
            note(0, 64, true),
            note(100, 64, false),
            note(260_000_000, 64, true),
            note(100, 64, false),
        ];
        assert!(matches!(
            import_midi(&write(PPQ, vec![events])),
            Err(MidiImportError::TooLong)
        ));
    }

    #[test]
    fn an_extreme_tempo_does_not_wrap_around() {
        let tempo = TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(1.into())),
        };
        let mut events = vec![tempo];
        events.extend(quarters(&[64]));
        let score = import(&write(PPQ, vec![events])).unwrap();
        assert_eq!(score.tempo_bpm, 999);
    }

    /// Small deterministic generator, so a failure can be replayed from its seed.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, below: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as u32) % below.max(1)
        }
    }

    fn random_file(seed: u64) -> Vec<u8> {
        let mut rng = Lcg(seed);
        let ppq = [1u16, 24, 96, 480, 960, 3, 7][rng.next(7) as usize];
        let mut tracks = Vec::new();

        for _ in 0..=rng.next(3) {
            let mut events = Vec::new();
            if rng.next(2) == 0 {
                events.push(TrackEvent {
                    delta: 0.into(),
                    kind: TrackEventKind::Meta(MetaMessage::TimeSignature(
                        rng.next(20) as u8,
                        rng.next(9) as u8,
                        24,
                        8,
                    )),
                });
            }
            if rng.next(2) == 0 {
                events.push(TrackEvent {
                    delta: 0.into(),
                    kind: TrackEventKind::Meta(MetaMessage::Tempo(rng.next(20_000_000).into())),
                });
            }
            for _ in 0..rng.next(60) {
                let delta = [0, 0, 1, 5, 40, 120, 480, 1000, 5000][rng.next(9) as usize];
                let channel = rng.next(16) as u8;
                let key = rng.next(128) as u8;
                events.push(match rng.next(5) {
                    0 | 1 => on_channel(channel, delta, key, true),
                    2 | 3 => on_channel(channel, delta, key, false),
                    _ => program(channel, rng.next(128) as u8),
                });
            }
            tracks.push(events);
        }
        write(ppq, tracks)
    }

    #[test]
    fn random_files_never_crash_and_always_give_a_coherent_score() {
        let mut imported = 0;
        let mut with_notes = 0;
        for seed in 0..250 {
            let Ok(result) = import_midi(&random_file(seed)) else {
                continue;
            };
            imported += 1;
            with_notes += usize::from(!result.score.tracks.is_empty());

            for track in &result.score.tracks {
                for measure in &track.measures {
                    assert!(measure.is_complete(), "seed {seed}: incomplete measure");
                    for beat in &measure.voices[0].beats {
                        let mut strings: Vec<u8> = beat.notes.iter().map(|n| n.string).collect();
                        strings.sort_unstable();
                        strings.dedup();
                        assert_eq!(
                            strings.len(),
                            beat.notes.len(),
                            "seed {seed}: shared string"
                        );
                        for n in &beat.notes {
                            assert!(track.pitch(n).is_some(), "seed {seed}: unplayable note");
                        }
                    }
                }
            }
        }
        eprintln!("{imported} files imported, {with_notes} with notes");
        // The test is meaningless if the generator mostly produces refused files.
        assert!(
            imported >= 120 && with_notes >= 60,
            "{imported} imported, {with_notes} with notes"
        );
    }
}
