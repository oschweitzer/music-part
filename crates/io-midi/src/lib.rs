//! Import MIDI -> `Score`.
//!
//! Limites actuelles : le modèle n'a pas de liaisons (ties), donc une note qui
//! traverse une barre de mesure ou dont la durée n'est pas représentable est
//! suivie de silences pour le reste de sa durée.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use score_model::{
    Beat, Duration, Instrument, Measure, Note, NoteValue, Score, TimeSignature, Track, Voice,
    GUITAR_STANDARD,
};

/// Plus petite unité : 1/128 de ronde. On quantifie à la triple-croche (4).
const GRID: u32 = 4;
const MAX_FRET: u8 = 24;

#[derive(Debug)]
pub enum MidiImportError {
    Parse(midly::Error),
    UnsupportedTiming,
}

impl fmt::Display for MidiImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "fichier MIDI invalide : {e}"),
            Self::UnsupportedTiming => write!(f, "timing SMPTE non supporté"),
        }
    }
}

impl std::error::Error for MidiImportError {}

impl From<midly::Error> for MidiImportError {
    fn from(e: midly::Error) -> Self {
        Self::Parse(e)
    }
}

pub fn import_midi(bytes: &[u8]) -> Result<Score, MidiImportError> {
    let smf = Smf::parse(bytes)?;
    let ppq = match smf.header.timing {
        Timing::Metrical(t) => u32::from(t.as_int()),
        Timing::Timecode(..) => return Err(MidiImportError::UnsupportedTiming),
    };

    let mut tempo_bpm = 120;
    let mut time_signature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };
    let mut title = None;
    let mut tempo_seen = false;
    let mut ts_seen = false;
    let mut tracks = Vec::new();

    for midi_track in &smf.tracks {
        let mut abs: u64 = 0;
        let mut open: HashMap<(u8, u8), u64> = HashMap::new();
        let mut notes: Vec<(u32, u32, u8)> = Vec::new();
        let mut name = None;

        for ev in midi_track {
            abs += u64::from(ev.delta.as_int());
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) if !tempo_seen => {
                    let us = us.as_int().max(1);
                    tempo_bpm = (60_000_000 / us) as u16;
                    tempo_seen = true;
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) if !ts_seen => {
                    time_signature = TimeSignature {
                        numerator: n.max(1),
                        denominator: 1 << d.min(7),
                    };
                    ts_seen = true;
                }
                TrackEventKind::Meta(MetaMessage::TrackName(n)) if name.is_none() => {
                    name = Some(String::from_utf8_lossy(n).into_owned());
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    let (key, on) = match message {
                        MidiMessage::NoteOn { key, vel } => (key.as_int(), vel.as_int() > 0),
                        MidiMessage::NoteOff { key, .. } => (key.as_int(), false),
                        _ => continue,
                    };
                    if on {
                        open.insert((ch, key), abs);
                    } else if let Some(start) = open.remove(&(ch, key)) {
                        let s = quantize(start, ppq);
                        let e = quantize(abs, ppq).max(s + GRID);
                        notes.push((s, e, key));
                    }
                }
                _ => {}
            }
        }

        if notes.is_empty() {
            continue;
        }
        if title.is_none() {
            title = name.clone();
        }
        tracks.push((
            name.unwrap_or_else(|| format!("Track {}", tracks.len() + 1)),
            notes,
        ));
    }

    let tracks = tracks
        .into_iter()
        .map(|(name, notes)| build_track(name, &notes, time_signature))
        .collect();

    Ok(Score {
        title: title.unwrap_or_else(|| "Untitled".into()),
        tempo_bpm,
        tracks,
    })
}

/// Ticks MIDI -> unités de 1/128 de ronde, arrondi à la grille.
fn quantize(tick: u64, ppq: u32) -> u32 {
    let units = (tick * 32 + u64::from(ppq) / 2) / u64::from(ppq);
    (units as u32 + GRID / 2) / GRID * GRID
}

/// Choisit (corde, frette) avec la frette la plus basse.
fn fret_position(tuning: &[u8], pitch: u8) -> Option<(u8, u8)> {
    tuning
        .iter()
        .enumerate()
        .filter_map(|(i, &open)| {
            let fret = pitch.checked_sub(open)?;
            (fret <= MAX_FRET).then_some((i as u8 + 1, fret))
        })
        .min_by_key(|&(_, fret)| fret)
}

/// Découpe une longueur en durées représentables, de la plus grande à la plus petite.
fn decompose(mut len: u32) -> Vec<Duration> {
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
        .collect();
    candidates.sort_by_key(|d| std::cmp::Reverse(d.ticks()));

    let mut out = Vec::new();
    while len > 0 {
        let d = *candidates
            .iter()
            .find(|d| d.ticks() <= len)
            .expect("len >= GRID");
        len -= d.ticks();
        out.push(d);
    }
    out
}

fn build_track(name: String, notes: &[(u32, u32, u8)], ts: TimeSignature) -> Track {
    let tuning = GUITAR_STANDARD.to_vec();

    // Regroupe par début : accords.
    let mut by_start: BTreeMap<u32, Vec<(u32, u8)>> = BTreeMap::new();
    for &(s, e, p) in notes {
        by_start.entry(s).or_default().push((e, p));
    }

    // Segments (début, longueur, notes) avec silences intercalés.
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
        let chord = group
            .iter()
            .filter_map(|&(_, p)| fret_position(&tuning, p))
            .map(|(string, fret)| Note {
                string,
                fret,
                technique: None,
            })
            .collect();
        segments.push((start, end - start, chord));
        cursor = end;
        if let Some(&next) = starts.get(i + 1) {
            if next > cursor {
                segments.push((cursor, next - cursor, vec![]));
                cursor = next;
            }
        }
    }

    // Découpe en mesures.
    let measure_len = ts.ticks();
    let total = segments.last().map_or(0, |&(s, l, _)| s + l);
    let measure_count = total.div_ceil(measure_len).max(1);
    let mut measures: Vec<Vec<Beat>> = vec![Vec::new(); measure_count as usize];

    for (start, len, chord) in segments {
        let mut pos = start;
        let end = start + len;
        let mut first = true;
        while pos < end {
            let m = (pos / measure_len) as usize;
            let piece_end = end.min((m as u32 + 1) * measure_len);
            // Les morceaux après le premier sont des silences (pas de ties).
            let notes = if first { chord.clone() } else { vec![] };
            for (j, duration) in decompose(piece_end - pos).into_iter().enumerate() {
                let notes = if j == 0 { notes.clone() } else { vec![] };
                measures[m].push(Beat { duration, notes });
            }
            first = false;
            pos = piece_end;
        }
    }

    // Complète la dernière mesure avec un silence.
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

    Track {
        name,
        instrument: Instrument::Guitar,
        tuning,
        measures: measures
            .into_iter()
            .map(|beats| Measure {
                time_signature: ts,
                voices: vec![Voice { beats }],
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{num::u4, Format, Header, MetaMessage, TrackEvent};

    const PPQ: u16 = 480;

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
        let score = import_midi(&smf_bytes(quarters(&[64, 59, 55, 40]))).unwrap();
        assert_eq!(score.tempo_bpm, 120);
        assert_eq!(score.title, "Lead");
        let track = &score.tracks[0];
        assert_eq!(track.measures.len(), 1);
        assert!(track.measures[0].is_complete());
        let beats = &track.measures[0].voices[0].beats;
        // 64 = Mi aigu à vide (corde 1), 40 = Mi grave à vide (corde 6).
        assert_eq!(
            beats[0].notes,
            vec![Note {
                string: 1,
                fret: 0,
                technique: None
            }]
        );
        assert_eq!(
            beats[3].notes,
            vec![Note {
                string: 6,
                fret: 0,
                technique: None
            }]
        );
    }

    #[test]
    fn gap_becomes_rest_and_measures_split() {
        // 4 noires, puis silence d'une noire, puis une noire : 2 mesures.
        let mut ev = quarters(&[64, 64, 64, 64]);
        ev.extend([note(480, 64, true), note(480, 64, false)]);
        let score = import_midi(&smf_bytes(ev)).unwrap();
        let measures = &score.tracks[0].measures;
        assert_eq!(measures.len(), 2);
        assert!(measures.iter().all(Measure::is_complete));
        assert!(measures[1].voices[0].beats[0].notes.is_empty());
        assert_eq!(measures[1].voices[0].beats[1].notes.len(), 1);
    }

    #[test]
    fn chord_keeps_all_notes() {
        let ev = vec![
            note(0, 64, true),
            note(0, 55, true),
            note(1920, 64, false),
            note(0, 55, false),
        ];
        let score = import_midi(&smf_bytes(ev)).unwrap();
        let first = &score.tracks[0].measures[0].voices[0].beats[0];
        assert_eq!(first.notes.len(), 2);
    }

    #[test]
    fn invalid_data_is_an_error() {
        assert!(import_midi(b"not midi").is_err());
    }
}
