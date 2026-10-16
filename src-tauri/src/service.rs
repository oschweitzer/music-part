//! What each command does to the document, without anything specific to Tauri. The app wraps these
//! functions in commands; the end-to-end test backend (`examples/headless.rs`) calls them directly,
//! so both speak exactly the same JSON.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::document::{Document, DocumentState};
use crate::files;
use audio_engine::{pack, DeclaredSound, PackError, PackFailure, SoundPack};
use score_model::{blank_score, demo_score, Edit, Sound};

/// A document that was just opened, with what the UI should tell the user about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDocument {
    pub document: DocumentState,
    /// Notes a string instrument could not play, left out when importing a MIDI file.
    pub skipped_notes: usize,
    /// Why the requested file could not be opened and the demo is shown instead.
    pub notice: Option<String>,
}

/// What happened when the app started, kept until the UI asks for it.
pub struct Startup {
    pub skipped_notes: usize,
    pub notice: Option<String>,
}

/// The file given on the command line (or by "Open with"), or the demo score.
pub fn initial_document(args: impl Iterator<Item = String>) -> (Document, Startup) {
    let demo = || Document::new(demo_score(), None);
    let Some(path) = files::file_from_args(args) else {
        return (
            demo(),
            Startup {
                skipped_notes: 0,
                notice: None,
            },
        );
    };
    match files::open_path(&path) {
        Ok(opened) => (
            opened.document,
            Startup {
                skipped_notes: opened.skipped_notes,
                notice: None,
            },
        ),
        Err(reason) => (
            demo(),
            Startup {
                skipped_notes: 0,
                notice: Some(reason),
            },
        ),
    }
}

pub fn startup(document: &Document, startup: &Startup) -> OpenedDocument {
    OpenedDocument {
        document: document.state(),
        skipped_notes: startup.skipped_notes,
        notice: startup.notice.clone(),
    }
}

pub fn new_document(document: &mut Document) -> DocumentState {
    *document = Document::new(blank_score("Untitled"), None);
    document.state()
}

pub fn open_file(document: &mut Document, path: &str) -> Result<OpenedDocument, String> {
    let opened = files::open_path(Path::new(path))?;
    *document = opened.document;
    Ok(OpenedDocument {
        document: document.state(),
        skipped_notes: opened.skipped_notes,
        notice: None,
    })
}

/// Saves to `path`, or to the file the document already has when `path` is omitted.
pub fn save_file(document: &mut Document, path: Option<String>) -> Result<DocumentState, String> {
    let target = match (path, document.path()) {
        (Some(chosen), _) => files::score_path(Path::new(&chosen)),
        (None, Some(current)) => current.clone(),
        (None, None) => return Err("This score has no file yet: choose where to save it.".into()),
    };
    files::save_score(&target, document.score())?;
    document.mark_saved(target);
    Ok(document.state())
}

/// Writes the score in another format. The document is untouched: it is not renamed, nor saved.
/// Returns the path written, which has the extension of the format.
pub fn export_file(
    document: &Document,
    path: &str,
    format: files::ExportFormat,
) -> Result<String, String> {
    let target = files::export_score(Path::new(path), format, document.score())?;
    Ok(target.display().to_string())
}

/// Applies an edit. A sound that is not built in must be one a pack found at start declares (or
/// the sound the track already has, whose pack may be missing); the track then records the
/// General MIDI program the pack declared, so the score still plays where the pack is not
/// installed.
pub fn edit_score(
    document: &mut Document,
    edit: &Edit,
    packs: &FoundPacks,
) -> Result<DocumentState, String> {
    let edit = with_fallback_program(document, edit, packs)?;
    document.edit(&edit).map_err(|e| e.to_string())?;
    Ok(document.state())
}

/// `edit` with the fallback program of a pack's sound filled in from the pack that declares it.
fn with_fallback_program(
    document: &Document,
    edit: &Edit,
    packs: &FoundPacks,
) -> Result<Edit, String> {
    let (sound, track) = match edit {
        Edit::SetSound { sound, track, .. } => (*sound, Some(*track)),
        Edit::AddTrack {
            sound: Some(sound), ..
        } => (*sound, None),
        _ => return Ok(edit.clone()),
    };
    if sound.is_built_in() {
        return Ok(edit.clone());
    }
    let declared = packs.declared();
    let program = match declared.iter().find(|d| d.sound == sound) {
        Some(declared) => Some(declared.program),
        None => {
            // Choosing again the sound of a track whose pack is missing keeps what it has.
            let current = track.and_then(|t| document.score().tracks.get(t));
            match current {
                Some(current) if current.sound == sound => current.fallback_program,
                _ => return Err(format!("no sound pack declares the sound \"{sound}\"")),
            }
        }
    };
    let mut edit = edit.clone();
    match &mut edit {
        Edit::SetSound {
            fallback_program, ..
        }
        | Edit::AddTrack {
            fallback_program, ..
        } => *fallback_program = program,
        _ => {}
    }
    Ok(edit)
}

pub fn undo(document: &mut Document) -> DocumentState {
    document.undo();
    document.state()
}

pub fn redo(document: &mut Document) -> DocumentState {
    document.redo();
    document.state()
}

/// The sound packs found at start. Their manifests are read once; the audio engine reads a pack's
/// samples only when the score uses one of its sounds.
#[derive(Default)]
pub struct FoundPacks {
    /// In order of priority, lowest first.
    pub packs: Vec<SoundPack>,
    /// The pack folders that could not be used.
    pub refused: Vec<PackError>,
    /// Where the user adds packs, when the app knows it.
    pub user_folder: Option<PathBuf>,
}

impl FoundPacks {
    /// Reads the manifests of the packs in `roots`, lowest priority first.
    pub fn discover(roots: &[PathBuf], user_folder: Option<PathBuf>) -> Self {
        let (packs, refused) = pack::discover(roots);
        Self {
            packs,
            refused,
            user_folder,
        }
    }

    /// The new sounds the packs declare: with the built-in table, the sounds a track can be given.
    pub fn declared(&self) -> Vec<DeclaredSound> {
        pack::declared_sounds(&self.packs)
    }
}

/// What the UI shows about the sound packs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundPacks {
    /// The packs in use, lowest priority first: when two play the same sound, the later one is
    /// heard.
    pub packs: Vec<PackInfo>,
    pub refused: Vec<RefusedPack>,
    pub user_folder: Option<String>,
}

/// A pack in use, as the UI lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    /// SPDX identifier of the licence of the samples.
    pub licence: String,
    pub source: Option<String>,
    pub folder: String,
    /// The sounds the pack plays. A sound whose sample file could not be used is left out: it
    /// plays with another pack or its General MIDI program, and is listed in `refused`.
    pub sounds: Vec<PackSoundInfo>,
}

/// A sound a pack plays, and the keys its recordings cover.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSoundInfo {
    pub sound: Sound,
    pub first_key: u8,
    pub last_key: u8,
    /// For a new sound the pack declares, its name; `None` for a sound of the built-in table.
    pub name: Option<String>,
    /// For a new sound, the General MIDI program that plays it without the pack.
    pub program: Option<u8>,
}

/// A pack folder, or one file of a pack, that could not be used, and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefusedPack {
    pub folder: String,
    pub reason: String,
}

/// The packs found at start, less the sample files the audio engine could not use (`failures`).
pub fn sound_packs(found: &FoundPacks, failures: &[PackFailure]) -> SoundPacks {
    let packs = found
        .packs
        .iter()
        .map(|pack| {
            let manifest = &pack.manifest;
            let has_failed = |sound: Sound| {
                failures
                    .iter()
                    .any(|f| f.pack == manifest.id && f.sound == sound)
            };
            PackInfo {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                author: manifest.author.clone(),
                licence: manifest.licence.clone(),
                source: manifest.source.clone(),
                folder: pack.dir.display().to_string(),
                sounds: manifest
                    .sounds
                    .iter()
                    .filter(|s| !has_failed(s.sound))
                    .map(|s| PackSoundInfo {
                        sound: s.sound,
                        first_key: s.first_key,
                        last_key: s.last_key,
                        name: s.name.clone(),
                        program: s.program,
                    })
                    .collect(),
            }
        })
        .collect();
    let refused = found
        .refused
        .iter()
        .chain(failures.iter().map(|f| &f.error))
        .map(|error| RefusedPack {
            folder: error.dir.display().to_string(),
            reason: error.reason.clone(),
        })
        .collect();
    SoundPacks {
        packs,
        refused,
        user_folder: found
            .user_folder
            .as_ref()
            .map(|dir| dir.display().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/tauri-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn demo() -> Document {
        Document::new(demo_score(), None)
    }

    #[test]
    fn without_a_file_argument_the_demo_opens_silently() {
        let (document, startup) = initial_document(["app".to_string()].into_iter());
        assert_eq!(document.score(), &demo_score());
        assert_eq!((startup.skipped_notes, startup.notice), (0, None));
    }

    #[test]
    fn a_new_document_replaces_the_current_one_and_is_clean() {
        let mut document = demo();
        edit_score(
            &mut document,
            &Edit::SetTempo { bpm: 80 },
            &FoundPacks::default(),
        )
        .unwrap();
        let state = new_document(&mut document);

        assert_eq!(state.score.title, "Untitled");
        assert!(!state.is_dirty && !state.can_undo && state.path.is_none());
    }

    #[test]
    fn a_refused_edit_comes_back_as_a_readable_message() {
        let mut document = demo();
        let error = edit_score(
            &mut document,
            &Edit::SetTempo { bpm: 0 },
            &FoundPacks::default(),
        )
        .unwrap_err();
        assert_eq!(error, "the tempo must be between 1 and 999 bpm");
    }

    #[test]
    fn saving_without_a_file_asks_for_one_and_then_remembers_it() {
        let mut document = demo();
        let error = save_file(&mut document, None).unwrap_err();
        assert!(error.contains("choose where to save"), "{error}");

        let path = scratch("service-save")
            .join("song")
            .to_string_lossy()
            .into_owned();
        let state = save_file(&mut document, Some(path.clone())).unwrap();
        assert_eq!(
            state.path.as_deref(),
            Some(format!("{path}.mpart").as_str())
        );

        edit_score(
            &mut document,
            &Edit::SetTempo { bpm: 77 },
            &FoundPacks::default(),
        )
        .unwrap();
        assert!(document.state().is_dirty);
        assert!(
            !save_file(&mut document, None).unwrap().is_dirty,
            "saves to the same file"
        );
    }

    #[test]
    fn a_saved_and_reopened_score_keeps_the_edits() {
        let mut document = demo();
        edit_score(
            &mut document,
            &Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 0,
                string: 1,
                fret: 12,
            },
            &FoundPacks::default(),
        )
        .unwrap();
        let path = scratch("service-roundtrip")
            .join("song.mpart")
            .to_string_lossy()
            .into_owned();
        save_file(&mut document, Some(path.clone())).unwrap();

        let mut other = demo();
        let opened = open_file(&mut other, &path).unwrap();
        assert_eq!(opened.document.score, *document.score());
        assert!(!opened.document.is_dirty);
    }

    #[test]
    fn opening_a_missing_file_leaves_the_current_document_alone() {
        let mut document = demo();
        assert!(open_file(&mut document, "/nonexistent/song.mpart").is_err());
        assert_eq!(document.score(), &demo_score());
    }

    #[test]
    fn undo_and_redo_report_the_state_even_when_there_is_nothing_to_do() {
        let mut document = demo();
        assert!(!undo(&mut document).can_undo);
        assert!(!redo(&mut document).can_redo);
    }

    fn write_pack(root: &Path, id: &str, manifest: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(pack::MANIFEST), manifest).unwrap();
    }

    const NYLON: &str = r#"{ "format": 1, "id": "nylon", "name": "Nylon", "author": "Me",
        "licence": "CC0-1.0", "sounds": [
            { "sound": "guitar/nylon", "file": "nylon.sf2", "firstKey": 40, "lastKey": 88 },
            { "sound": "guitar/steel", "file": "steel.sf2", "firstKey": 40, "lastKey": 88 }
        ] }"#;

    #[test]
    fn the_packs_list_what_they_play_and_the_refused_ones_say_why() {
        let root = scratch("service-packs");
        write_pack(&root, "nylon", NYLON);
        write_pack(&root, "broken", "{ not json");
        let found = FoundPacks::discover(std::slice::from_ref(&root), Some(root.join("user")));

        let packs = sound_packs(&found, &[]);

        assert_eq!(packs.packs.len(), 1);
        assert_eq!(packs.packs[0].licence, "CC0-1.0");
        assert_eq!(packs.packs[0].sounds.len(), 2);
        assert_eq!(packs.refused.len(), 1);
        assert!(packs.refused[0].folder.ends_with("broken"));
        assert!(packs.user_folder.unwrap().ends_with("user"));
    }

    #[test]
    fn a_sound_whose_file_failed_is_refused_and_no_longer_listed_by_its_pack() {
        let root = scratch("service-pack-failure");
        write_pack(&root, "nylon", NYLON);
        let found = FoundPacks::discover(std::slice::from_ref(&root), None);
        let failure = PackFailure {
            pack: "nylon".into(),
            sound: Sound::GUITAR_STEEL,
            error: PackError {
                dir: root.join("nylon"),
                reason: "steel.sf2: missing".into(),
            },
        };

        let packs = sound_packs(&found, &[failure]);

        let sounds: Vec<Sound> = packs.packs[0].sounds.iter().map(|s| s.sound).collect();
        assert_eq!(sounds, [Sound::GUITAR_NYLON]);
        assert_eq!(packs.refused[0].reason, "steel.sf2: missing");
    }

    const TWELVE: &str = r#"{ "format": 1, "id": "twelve", "name": "Twelve", "author": "Me",
        "licence": "CC0-1.0", "sounds": [
            { "sound": "guitar/twelve-string", "name": "Twelve-string", "program": 25,
              "file": "twelve.sfz", "firstKey": 40, "lastKey": 88 }
        ] }"#;

    fn twelve_string() -> Sound {
        Sound::parse("guitar/twelve-string").unwrap()
    }

    fn set_twelve_string() -> Edit {
        Edit::SetSound {
            track: 0,
            sound: twelve_string(),
            fallback_program: None,
        }
    }

    #[test]
    fn a_new_sound_of_a_pack_is_listed_with_its_name_and_program() {
        let root = scratch("service-new-sound");
        write_pack(&root, "twelve", TWELVE);
        let found = FoundPacks::discover(std::slice::from_ref(&root), None);

        let sound = &sound_packs(&found, &[]).packs[0].sounds[0];

        assert_eq!(sound.sound, twelve_string());
        assert_eq!(
            (sound.name.as_deref(), sound.program),
            (Some("Twelve-string"), Some(25))
        );
    }

    #[test]
    fn a_track_given_a_pack_sound_records_the_program_the_pack_declares() {
        let root = scratch("service-set-new-sound");
        write_pack(&root, "twelve", TWELVE);
        let found = FoundPacks::discover(std::slice::from_ref(&root), None);
        let mut document = demo();

        let state = edit_score(&mut document, &set_twelve_string(), &found).unwrap();

        let track = &state.score.tracks[0];
        assert_eq!(
            (track.sound, track.fallback_program),
            (twelve_string(), Some(25))
        );
    }

    #[test]
    fn a_sound_no_pack_declares_is_refused_unless_the_track_already_has_it() {
        let mut document = demo();
        let none = FoundPacks::default();
        let error = edit_score(&mut document, &set_twelve_string(), &none).unwrap_err();
        assert!(error.contains("no sound pack declares"), "{error}");
        let add = Edit::AddTrack {
            instrument: score_model::Instrument::Guitar,
            sound: Some(twelve_string()),
            fallback_program: Some(25),
        };
        assert!(edit_score(&mut document, &add, &none).is_err());

        // A score made with the pack, opened without it.
        let mut score = demo_score();
        score.tracks[0].sound = twelve_string();
        score.tracks[0].fallback_program = Some(25);
        let mut document = Document::new(score, None);
        let state = edit_score(&mut document, &set_twelve_string(), &none).unwrap();
        assert_eq!(state.score.tracks[0].fallback_program, Some(25));
    }

    #[test]
    fn the_json_the_ui_receives_is_camel_case() {
        let json = serde_json::to_value(startup(
            &demo(),
            &Startup {
                skipped_notes: 2,
                notice: None,
            },
        ))
        .unwrap();
        assert_eq!(json["skippedNotes"], 2);
        assert_eq!(json["document"]["isDirty"], false);
        assert_eq!(json["document"]["canUndo"], false);
        assert!(json["notice"].is_null());
        // The score itself keeps the model's own field names.
        assert!(json["document"]["score"]["tempo_bpm"].is_number());
    }
}
