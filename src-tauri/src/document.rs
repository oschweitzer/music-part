//! The document being edited: the score, its undo history, and the file it is saved in.

use std::path::PathBuf;

use score_model::{apply_edit, Edit, EditError, Score};
use serde::Serialize;

/// Oldest steps are forgotten beyond this many.
const MAX_UNDO: usize = 200;

pub struct Document {
    score: Score,
    undo: Vec<Score>,
    redo: Vec<Score>,
    path: Option<PathBuf>,
    /// Length of `undo` when the document matched its file. `None` when it never did, or when
    /// that state can no longer be reached (an edit after undoing past it).
    saved_at: Option<usize>,
}

/// What the UI needs to show the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentState {
    pub score: Score,
    pub path: Option<String>,
    pub is_dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl Document {
    /// A document that is clean: nothing has been edited yet, so nothing can be lost.
    pub fn new(score: Score, path: Option<PathBuf>) -> Self {
        Self {
            score,
            undo: Vec::new(),
            redo: Vec::new(),
            path,
            saved_at: Some(0),
        }
    }

    pub fn score(&self) -> &Score {
        &self.score
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn state(&self) -> DocumentState {
        DocumentState {
            score: self.score.clone(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            is_dirty: self.saved_at != Some(self.undo.len()),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }

    /// Applies an edit. An edit that changes nothing is not recorded in the history.
    pub fn edit(&mut self, edit: &Edit) -> Result<(), EditError> {
        let mut next = self.score.clone();
        apply_edit(&mut next, edit)?;
        if next == self.score {
            return Ok(());
        }

        // If the saved state sits in the redo branch that is about to be dropped, it is gone.
        if self.saved_at.is_some_and(|saved| saved > self.undo.len()) {
            self.saved_at = None;
        }
        self.undo.push(std::mem::replace(&mut self.score, next));
        self.redo.clear();

        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
            self.saved_at = self.saved_at.and_then(|saved| saved.checked_sub(1));
        }
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.score, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.score, next));
        true
    }

    /// Records that the document now matches the file at `path`.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.saved_at = Some(self.undo.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::{blank_score, Note};

    fn fret(measure: usize, string: u8, fret: u8) -> Edit {
        Edit::SetFret {
            track: 0,
            measure,
            beat: 0,
            string,
            fret,
        }
    }

    fn first_note(document: &Document, measure: usize) -> Option<Note> {
        document.score().tracks[0].measures[measure].voices[0].beats[0]
            .notes
            .first()
            .cloned()
    }

    fn blank() -> Document {
        Document::new(blank_score("t"), None)
    }

    #[test]
    fn a_new_document_is_clean_and_has_no_history() {
        let state = blank().state();
        assert!(!state.is_dirty && !state.can_undo && !state.can_redo);
    }

    #[test]
    fn an_edit_makes_it_dirty_and_undo_restores_the_previous_score() {
        let mut document = blank();
        let before = document.score().clone();
        document.edit(&fret(0, 1, 5)).unwrap();

        assert!(document.state().is_dirty);
        assert!(document.state().can_undo);
        assert!(document.undo());
        assert_eq!(document.score(), &before);
        assert!(
            !document.state().is_dirty,
            "undoing back to the start is clean again"
        );
    }

    #[test]
    fn redo_replays_what_was_undone_and_a_new_edit_clears_it() {
        let mut document = blank();
        document.edit(&fret(0, 1, 5)).unwrap();
        document.undo();
        assert!(document.state().can_redo);

        assert!(document.redo());
        assert_eq!(first_note(&document, 0).map(|n| n.fret), Some(5));
        assert!(!document.redo(), "nothing left to redo");

        document.undo();
        document.edit(&fret(0, 1, 7)).unwrap();
        assert!(!document.state().can_redo);
    }

    #[test]
    fn undo_and_redo_with_empty_history_do_nothing() {
        let mut document = blank();
        assert!(!document.undo());
        assert!(!document.redo());
    }

    #[test]
    fn an_edit_that_changes_nothing_is_not_recorded() {
        let mut document = blank();
        document.edit(&fret(0, 1, 5)).unwrap();
        document.edit(&fret(0, 1, 5)).unwrap();
        document
            .edit(&Edit::RemoveNote {
                track: 0,
                measure: 0,
                beat: 0,
                string: 6,
            })
            .unwrap();

        assert!(document.undo());
        assert!(!document.state().can_undo, "only one step was recorded");
    }

    #[test]
    fn a_refused_edit_changes_nothing_and_is_not_recorded() {
        let mut document = blank();
        assert!(document.edit(&fret(0, 1, 99)).is_err());
        assert!(!document.state().can_undo && !document.state().is_dirty);
    }

    #[test]
    fn saving_makes_it_clean_and_remembers_the_path() {
        let mut document = blank();
        document.edit(&fret(0, 1, 5)).unwrap();
        document.mark_saved(PathBuf::from("/music/song.mpart"));

        let state = document.state();
        assert!(!state.is_dirty);
        assert_eq!(state.path.as_deref(), Some("/music/song.mpart"));

        document.edit(&fret(0, 2, 3)).unwrap();
        assert!(document.state().is_dirty);
        document.undo();
        assert!(!document.state().is_dirty, "back at the saved state");
        document.undo();
        assert!(document.state().is_dirty, "before the saved state");
    }

    #[test]
    fn editing_after_undoing_past_the_saved_state_cannot_look_clean() {
        let mut document = blank();
        document.edit(&fret(0, 1, 5)).unwrap();
        document.mark_saved(PathBuf::from("/music/song.mpart"));
        document.undo();
        // A different edit now reaches the same history length as the saved state.
        document.edit(&fret(0, 2, 9)).unwrap();

        assert!(document.state().is_dirty);
    }

    #[test]
    fn the_history_is_capped_and_the_saved_marker_follows() {
        let mut document = blank();
        document.mark_saved(PathBuf::from("/music/song.mpart"));
        for i in 0..(MAX_UNDO + 20) {
            document
                .edit(&fret(0, 1, (i % 25) as u8 + u8::from(i % 25 == 0)))
                .unwrap();
        }
        let mut steps = 0;
        while document.undo() {
            steps += 1;
        }
        assert_eq!(steps, MAX_UNDO);
        assert!(
            document.state().is_dirty,
            "the saved state fell out of the history"
        );
    }
}
