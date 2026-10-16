//! The native score file (`.mpart`): the score as JSON inside an envelope that names the format and
//! its version, so that the model can evolve without breaking old files.
//!
//! ```json
//! { "format": "music-part", "version": 1, "score": { ... } }
//! ```
//!
//! Compatibility rule: a change that older readers can still load keeps the version (new fields
//! take `#[serde(default)]`); a change that cannot bumps `VERSION` and gets a migration here.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use score_model::Score;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "music-part";
pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "mpart";

#[derive(Debug)]
pub enum NativeError {
    /// Not JSON, or JSON that is not a Music Part file.
    NotAScoreFile,
    /// Written by a newer version of the program.
    TooNew {
        found: u32,
    },
    /// A Music Part file whose score is damaged or breaks the model's rules.
    Invalid(String),
    Io(std::io::Error),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAScoreFile => write!(f, "this is not a Music Part score file"),
            Self::TooNew { found } => write!(
                f,
                "this file was saved by a newer version of Music Part (file format {found}, this version reads up to {VERSION})"
            ),
            Self::Invalid(reason) => write!(f, "this score file is damaged: {reason}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<std::io::Error> for NativeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'a str,
    version: u32,
    score: &'a Score,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    format: String,
    version: u32,
    score: serde_json::Value,
}

pub fn to_string(score: &Score) -> String {
    let envelope = EnvelopeOut {
        format: FORMAT,
        version: VERSION,
        score,
    };
    serde_json::to_string(&envelope).expect("a score always serializes")
}

/// Parses and validates a score file.
pub fn from_str(text: &str) -> Result<Score, NativeError> {
    let envelope: EnvelopeIn =
        serde_json::from_str(text).map_err(|_| NativeError::NotAScoreFile)?;
    if envelope.format != FORMAT {
        return Err(NativeError::NotAScoreFile);
    }
    if envelope.version > VERSION {
        return Err(NativeError::TooNew {
            found: envelope.version,
        });
    }

    let score: Score =
        serde_json::from_value(envelope.score).map_err(|e| NativeError::Invalid(e.to_string()))?;
    score
        .validate()
        .map_err(|e| NativeError::Invalid(e.to_string()))?;
    Ok(score)
}

pub fn read_file(path: &Path) -> Result<Score, NativeError> {
    from_str(&fs::read_to_string(path)?)
}

/// Writes next to the target first and renames, so that a crash or a full disk never leaves a
/// half-written file where a good one used to be.
pub fn write_file(path: &Path, score: &Score) -> Result<(), NativeError> {
    let mut temporary: PathBuf = path.to_path_buf();
    let mut name = temporary
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".saving");
    temporary.set_file_name(name);

    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(to_string(score).as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(NativeError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::{
        apply_edit, blank_score, demo_score, Clef, Edit, Instrument, Sound, DRUM_PIECES,
    };

    #[test]
    fn a_score_survives_a_round_trip() {
        for score in [demo_score(), blank_score("Mine")] {
            assert_eq!(from_str(&to_string(&score)).unwrap(), score);
        }
    }

    #[test]
    fn every_sound_survives_a_round_trip() {
        let mut score = blank_score("Sounds");
        for sound in Sound::ALL {
            let edit = Edit::AddTrack {
                instrument: sound.instrument(),
                sound: Some(sound),
                fallback_program: None,
            };
            apply_edit(&mut score, &edit).unwrap();
        }
        let back = from_str(&to_string(&score)).unwrap();
        let sounds: Vec<Sound> = back.tracks.iter().skip(1).map(|t| t.sound).collect();
        assert_eq!(sounds, Sound::ALL);
        assert_eq!(back, score);
    }

    #[test]
    fn a_pack_sound_and_its_fallback_program_survive_a_round_trip_without_the_pack() {
        let mut score = demo_score();
        let edit = Edit::SetSound {
            track: 0,
            sound: Sound::parse("guitar/twelve-string").unwrap(),
            fallback_program: Some(25),
        };
        apply_edit(&mut score, &edit).unwrap();
        let text = to_string(&score);
        assert!(text.contains(r#""fallback_program":25"#), "{text}");
        assert_eq!(from_str(&text).unwrap(), score);
    }

    #[test]
    fn a_file_with_built_in_sounds_is_written_as_before() {
        assert!(!to_string(&demo_score()).contains("fallback_program"));
    }

    #[test]
    fn every_drum_piece_survives_a_round_trip() {
        let mut score = blank_score("Drums");
        let to_drums = Edit::SetInstrument {
            track: 0,
            instrument: Instrument::Drums,
        };
        apply_edit(&mut score, &to_drums).unwrap();
        for pitch in DRUM_PIECES {
            let hit = Edit::AddPitch {
                track: 0,
                measure: 0,
                beat: 0,
                pitch,
            };
            apply_edit(&mut score, &hit).unwrap();
        }
        let text = to_string(&score);
        assert!(text.contains(r#"{"piece":35,"technique":null,"tied":false}"#));
        let back = from_str(&text).unwrap();
        let keys: Vec<u8> = back.tracks[0].measures[0].voices[0].beats[0]
            .notes
            .iter()
            .map(|n| n.key())
            .collect();
        assert_eq!(keys, DRUM_PIECES.collect::<Vec<u8>>());
        assert_eq!(back, score);
    }

    #[test]
    fn the_clef_of_a_voice_survives_a_round_trip_and_other_tracks_write_none() {
        let mut score = blank_score("Choir");
        apply_edit(
            &mut score,
            &Edit::AddTrack {
                instrument: Instrument::Voice,
                sound: None,
                fallback_program: None,
            },
        )
        .unwrap();
        let clef = Edit::SetClef {
            track: 1,
            clef: Clef::Treble8vb,
        };
        apply_edit(&mut score, &clef).unwrap();
        let text = to_string(&score);
        assert_eq!(text.matches("\"clef\"").count(), 1, "{text}");
        assert!(text.contains("\"clef\":\"treble8vb\""), "{text}");
        assert_eq!(from_str(&text).unwrap(), score);
    }

    #[test]
    fn a_voice_written_without_a_clef_gets_the_treble_clef() {
        let mut score = blank_score("Choir");
        let add = Edit::AddTrack {
            instrument: Instrument::Voice,
            sound: None,
            fallback_program: None,
        };
        apply_edit(&mut score, &add).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&to_string(&score)).unwrap();
        json["score"]["tracks"][1]
            .as_object_mut()
            .unwrap()
            .remove("clef");
        let back = from_str(&json.to_string()).unwrap();
        assert_eq!(back.tracks[1].clef, Some(Clef::Treble));
    }

    #[test]
    fn a_file_saved_before_sounds_existed_gets_the_sound_it_had() {
        let mut json: serde_json::Value = serde_json::from_str(&to_string(&demo_score())).unwrap();
        for track in json["score"]["tracks"].as_array_mut().unwrap() {
            track.as_object_mut().unwrap().remove("sound");
        }
        let score = from_str(&json.to_string()).unwrap();
        assert_eq!(score.tracks[0].sound, Sound::GUITAR_NYLON);
        assert_eq!(score.tracks[1].sound, Sound::BASS_FINGER);
    }

    #[test]
    fn a_sound_of_another_instrument_is_refused() {
        let text = to_string(&demo_score()).replace("\"bass/finger\"", "\"violin/standard\"");
        assert!(matches!(from_str(&text), Err(NativeError::Invalid(_))));
    }

    #[test]
    fn the_envelope_names_the_format_and_version() {
        let json: serde_json::Value = serde_json::from_str(&to_string(&blank_score("t"))).unwrap();
        assert_eq!(json["format"], "music-part");
        assert_eq!(json["version"], 1);
    }

    #[test]
    fn other_json_and_garbage_are_not_score_files() {
        for text in [
            "",
            "not json",
            "{}",
            r#"{"format":"other","version":1,"score":{}}"#,
            "[1,2]",
        ] {
            assert!(
                matches!(from_str(text), Err(NativeError::NotAScoreFile)),
                "{text}"
            );
        }
    }

    #[test]
    fn a_file_from_the_future_says_so() {
        let text = r#"{"format":"music-part","version":99,"score":{}}"#;
        let error = from_str(text).unwrap_err();
        assert!(matches!(error, NativeError::TooNew { found: 99 }));
        assert!(error.to_string().contains("newer version"));
    }

    #[test]
    fn a_damaged_score_is_refused_with_a_reason() {
        let mut score = demo_score();
        score.tracks[0].measures[0].voices[0].beats.pop();
        let error = from_str(&to_string(&score)).unwrap_err();
        assert!(matches!(error, NativeError::Invalid(_)));
        assert!(error.to_string().contains("measure 1"), "{error}");

        let truncated = r#"{"format":"music-part","version":1,"score":{"title":"x"}}"#;
        assert!(matches!(from_str(truncated), Err(NativeError::Invalid(_))));
    }

    #[test]
    fn a_hand_edited_zero_denominator_is_refused_not_a_crash() {
        let text = to_string(&blank_score("t")).replace("\"denominator\":4", "\"denominator\":0");
        assert!(matches!(from_str(&text), Err(NativeError::Invalid(_))));
    }
}
