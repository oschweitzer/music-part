//! What each command does to the document, without anything specific to Tauri. The app wraps these
//! functions in commands; the end-to-end test backend (`examples/headless.rs`) calls them directly,
//! so both speak exactly the same JSON.

use std::path::Path;

use serde::Serialize;

use crate::document::{Document, DocumentState};
use crate::files;
use score_model::{blank_score, demo_score, Edit};

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

pub fn edit_score(document: &mut Document, edit: &Edit) -> Result<DocumentState, String> {
    document.edit(edit).map_err(|e| e.to_string())?;
    Ok(document.state())
}

pub fn undo(document: &mut Document) -> DocumentState {
    document.undo();
    document.state()
}

pub fn redo(document: &mut Document) -> DocumentState {
    document.redo();
    document.state()
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
        edit_score(&mut document, &Edit::SetTempo { bpm: 80 }).unwrap();
        let state = new_document(&mut document);

        assert_eq!(state.score.title, "Untitled");
        assert!(!state.is_dirty && !state.can_undo && state.path.is_none());
    }

    #[test]
    fn a_refused_edit_comes_back_as_a_readable_message() {
        let mut document = demo();
        let error = edit_score(&mut document, &Edit::SetTempo { bpm: 0 }).unwrap_err();
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

        edit_score(&mut document, &Edit::SetTempo { bpm: 77 }).unwrap();
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
