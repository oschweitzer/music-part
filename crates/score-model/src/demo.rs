//! A small hand-written score used by the UI until real import and editing exist.
//! The same score is exported to `ui/public/demo-score.json` so the UI can run in a plain
//! browser; the test below keeps that file in sync.

use crate::{
    Beat, Duration, Instrument, Measure, Note, NoteValue, Score, Technique, TimeSignature, Track,
    Voice, GUITAR_STANDARD,
};

const BASS_STANDARD: [u8; 4] = [43, 38, 33, 28];

fn note(string: u8, fret: u8) -> Note {
    Note {
        string,
        fret,
        technique: None,
        tied: false,
    }
}

fn beat(value: NoteValue, dots: u8, notes: Vec<Note>) -> Beat {
    Beat {
        duration: Duration { value, dots },
        notes,
    }
}

fn measure(beats: Vec<Beat>) -> Measure {
    Measure {
        time_signature: TimeSignature {
            numerator: 4,
            denominator: 4,
        },
        voices: vec![Voice { beats }],
    }
}

/// Eight eighth notes, one per `(string, fret)` pair.
fn eighths(notes: [(u8, u8); 8]) -> Measure {
    measure(
        notes
            .iter()
            .map(|&(s, f)| beat(NoteValue::Eighth, 0, vec![note(s, f)]))
            .collect(),
    )
}

fn quarters(string: u8, fret: u8) -> Measure {
    measure(
        (0..4)
            .map(|_| beat(NoteValue::Quarter, 0, vec![note(string, fret)]))
            .collect(),
    )
}

pub fn demo_score() -> Score {
    use NoteValue::{Eighth, Half, Quarter, Whole};

    let g_chord = vec![
        note(6, 3),
        note(5, 2),
        note(4, 0),
        note(3, 0),
        note(2, 0),
        note(1, 3),
    ];
    let a_minor = vec![note(5, 0), note(4, 2), note(3, 2), note(2, 1), note(1, 0)];

    let guitar_measures = vec![
        eighths([
            (5, 0),
            (4, 2),
            (3, 2),
            (2, 1),
            (1, 0),
            (2, 1),
            (3, 2),
            (4, 2),
        ]),
        eighths([
            (5, 3),
            (4, 2),
            (3, 0),
            (2, 1),
            (1, 0),
            (2, 1),
            (3, 0),
            (4, 2),
        ]),
        measure(vec![
            beat(Quarter, 1, g_chord),
            beat(Eighth, 0, vec![note(1, 3)]),
            beat(Quarter, 0, vec![note(3, 0)]),
            beat(
                Quarter,
                0,
                vec![Note {
                    technique: Some(Technique::HammerOn),
                    ..note(3, 2)
                }],
            ),
        ]),
        measure(vec![
            beat(Quarter, 0, vec![note(5, 0)]),
            beat(Quarter, 0, vec![note(4, 2)]),
            beat(Half, 0, vec![note(1, 0)]),
        ]),
        measure(vec![
            beat(
                Half,
                0,
                vec![Note {
                    tied: true,
                    ..note(1, 0)
                }],
            ),
            beat(
                Quarter,
                0,
                vec![Note {
                    technique: Some(Technique::Slide),
                    ..note(2, 1)
                }],
            ),
            beat(Quarter, 0, vec![]),
        ]),
        measure(vec![beat(Whole, 0, a_minor)]),
    ];

    let bass_measures = vec![
        quarters(3, 0),
        quarters(3, 3),
        quarters(4, 3),
        quarters(4, 0),
        measure(vec![beat(Whole, 0, vec![note(4, 0)])]),
        measure(vec![beat(Whole, 0, vec![note(3, 0)])]),
    ];

    Score {
        title: "Demo in A minor".into(),
        tempo_bpm: 100,
        tracks: vec![
            Track {
                name: "Guitar".into(),
                instrument: Instrument::Guitar,
                tuning: GUITAR_STANDARD.to_vec(),
                measures: guitar_measures,
            },
            Track {
                name: "Bass".into(),
                instrument: Instrument::Bass,
                tuning: BASS_STANDARD.to_vec(),
                measures: bass_measures,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_measures_are_complete_and_playable() {
        let score = demo_score();
        for track in &score.tracks {
            assert!(
                track.measures.iter().all(Measure::is_complete),
                "{}",
                track.name
            );
            let notes = track
                .measures
                .iter()
                .flat_map(|m| &m.voices[0].beats)
                .flat_map(|b| &b.notes);
            for n in notes {
                assert!(
                    track.pitch(n).is_some(),
                    "{}: string {} fret {}",
                    track.name,
                    n.string,
                    n.fret
                );
            }
        }
    }

    /// The UI loads this file when it runs outside Tauri. Regenerate with
    /// `UPDATE_DEMO=1 cargo test -p score-model demo_json`.
    #[test]
    fn demo_json_is_up_to_date() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/public/demo-score.json"
        );
        let expected = serde_json::to_string_pretty(&demo_score()).unwrap() + "\n";

        if std::env::var_os("UPDATE_DEMO").is_some() {
            std::fs::write(path, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(path).expect("ui/public/demo-score.json is missing");
        assert_eq!(
            actual, expected,
            "run UPDATE_DEMO=1 cargo test -p score-model demo_json"
        );
    }
}
