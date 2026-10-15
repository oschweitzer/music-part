//! Opening and saving score files.

use std::path::{Path, PathBuf};

use crate::document::Document;
use score_model::Score;

/// Largest MIDI file read. Real files are a few hundred kilobytes at most.
const MAX_MIDI_BYTES: u64 = 32 * 1024 * 1024;
/// Largest MusicXML file read, compressed or not.
const MAX_MUSICXML_BYTES: u64 = 64 * 1024 * 1024;
/// Largest score file read. A long piece is a few megabytes of JSON.
const MAX_SCORE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileKind {
    Midi,
    MusicXml,
    Native,
}

pub struct Opened {
    pub document: Document,
    /// Notes a string instrument could not play, left out when importing a MIDI file.
    pub skipped_notes: usize,
}

fn kind_of(path: &Path) -> Option<FileKind> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("mid") || extension.eq_ignore_ascii_case("midi") {
        Some(FileKind::Midi)
    } else if ["musicxml", "xml", "mxl"]
        .iter()
        .any(|e| extension.eq_ignore_ascii_case(e))
    {
        Some(FileKind::MusicXml)
    } else if extension.eq_ignore_ascii_case(io_native::EXTENSION) {
        Some(FileKind::Native)
    } else {
        None
    }
}

/// First argument that names an existing file we can open. Flags and the rest are ignored.
pub fn file_from_args(args: impl Iterator<Item = String>) -> Option<PathBuf> {
    args.skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from)
        .find(|path| kind_of(path).is_some() && path.is_file())
}

pub fn open_path(path: &Path) -> Result<Opened, String> {
    match kind_of(path) {
        Some(FileKind::Midi) => {
            let bytes = read_limited(path, MAX_MIDI_BYTES)?;
            let stem = path.file_stem().and_then(|s| s.to_str());
            let (score, skipped_notes) = import_midi_bytes(&bytes, stem)?;
            Ok(Opened {
                document: Document::new(score, None),
                skipped_notes,
            })
        }
        Some(FileKind::MusicXml) => {
            let bytes = read_limited(path, MAX_MUSICXML_BYTES)?;
            let stem = path.file_stem().and_then(|s| s.to_str());
            let (score, skipped_notes) = import_musicxml_bytes(&bytes, stem)?;
            Ok(Opened {
                document: Document::new(score, None),
                skipped_notes,
            })
        }
        Some(FileKind::Native) => {
            read_limited(path, MAX_SCORE_BYTES)?;
            let score = io_native::read_file(path).map_err(|e| e.to_string())?;
            Ok(Opened {
                document: Document::new(score, Some(path.to_path_buf())),
                skipped_notes: 0,
            })
        }
        None => Err(format!(
            "Cannot open {}: only .mpart, MIDI (.mid) and MusicXML (.musicxml, .xml, .mxl) files are supported.",
            path.display()
        )),
    }
}

/// Always a `.mpart` path: the dialog may leave the extension off, and a score must not be
/// written into a file that looks like something else.
pub fn score_path(path: &Path) -> PathBuf {
    path.with_extension(io_native::EXTENSION)
}

pub fn save_score(path: &Path, score: &Score) -> Result<(), String> {
    io_native::write_file(path, score).map_err(|e| format!("Cannot save {}: {e}", path.display()))
}

/// A format a score can be exported to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Midi,
    MusicXml,
}

impl ExportFormat {
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Midi => &["mid", "midi"],
            Self::MusicXml => &["musicxml", "xml"],
        }
    }
}

/// The path with the extension of the format added when it has none that fits: the dialog may
/// leave it off, and a MIDI file must not end up named like something else.
pub fn export_target(path: &Path, format: ExportFormat) -> PathBuf {
    let fits = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        format
            .extensions()
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    });
    if fits {
        path.to_path_buf()
    } else {
        path.with_extension(format.extensions()[0])
    }
}

/// Writes the score in another format. Returns where it went. Nothing is left half written.
pub fn export_score(path: &Path, format: ExportFormat, score: &Score) -> Result<PathBuf, String> {
    let target = export_target(path, format);
    let bytes = match format {
        ExportFormat::Midi => io_midi::export_midi(score).map_err(|e| e.to_string())?,
        ExportFormat::MusicXml => io_musicxml::export_musicxml(score).into_bytes(),
    };
    write_atomic(&target, &bytes)
        .map_err(|e| format!("Cannot export to {}: {e}", target.display()))?;
    Ok(target)
}

/// Writes next to the target and renames, so a failure never damages an existing file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let result = std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Size is checked before reading, so a wrong file cannot exhaust memory.
fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if size > limit {
        return Err(format!(
            "{} is too large to be a score file",
            path.display()
        ));
    }
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// `file_stem` names the score when the file does not carry a title.
fn import_midi_bytes(bytes: &[u8], file_stem: Option<&str>) -> Result<(Score, usize), String> {
    let mut imported = io_midi::import_midi(bytes).map_err(|e| e.to_string())?;
    if imported.score.tracks.is_empty() {
        return Err("This file has no notes to import (drum parts are not imported yet).".into());
    }
    if let (Some(stem), "Untitled") = (file_stem, imported.score.title.as_str()) {
        imported.score.title = stem.to_string();
    }
    Ok((imported.score, imported.skipped_notes))
}

/// `file_stem` names the score when the file does not carry a title.
fn import_musicxml_bytes(bytes: &[u8], file_stem: Option<&str>) -> Result<(Score, usize), String> {
    let mut imported = io_musicxml::import_musicxml(bytes).map_err(|e| e.to_string())?;
    if imported.score.title.trim().is_empty() {
        imported.score.title = file_stem.unwrap_or("Untitled").to_string();
    }
    Ok((imported.score, imported.skipped_notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::demo_score;

    /// One quarter note (E4) at 480 ticks per quarter, written by hand.
    const ONE_NOTE_MIDI: &[u8] = &[
        0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0, // header
        0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 13, // track, 13 bytes
        0x00, 0x90, 0x40, 0x64, // note on E4
        0x83, 0x60, 0x80, 0x40, 0x00, // note off after 480 ticks
        0x00, 0xFF, 0x2F, 0x00, // end of track
    ];

    /// A fresh directory under `target/`, so tests never write outside the repository.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/tauri-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    const ONE_NOTE_MUSICXML: &str = r#"<?xml version="1.0"?>
<score-partwise><part-list><score-part id="P1"><part-name>Guitar</part-name></score-part></part-list>
<part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
<note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration></note></measure></part></score-partwise>"#;

    #[test]
    fn imports_a_musicxml_file_and_names_it_after_the_file_when_it_has_no_title() {
        let (score, skipped) =
            import_musicxml_bytes(ONE_NOTE_MUSICXML.as_bytes(), Some("my-tune")).unwrap();
        assert_eq!(score.title, "my-tune");
        assert_eq!(score.tracks.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn opens_musicxml_files_by_extension_and_explains_a_bad_one() {
        let dir = scratch("musicxml");
        for name in ["tune.musicxml", "tune.XML"] {
            let path = dir.join(name);
            std::fs::write(&path, ONE_NOTE_MUSICXML).unwrap();
            let opened = open_path(&path).unwrap();
            assert_eq!(opened.document.score().title, "tune");
            assert!(opened.document.path().is_none());
        }

        let bad = dir.join("bad.mxl");
        std::fs::write(&bad, b"PK not an archive").unwrap();
        assert!(open_path(&bad).err().unwrap().contains("cannot read"));
        let html = dir.join("page.xml");
        std::fs::write(&html, "<html/>").unwrap();
        assert!(open_path(&html).err().unwrap().contains("not a MusicXML"));
    }

    #[test]
    fn exports_add_the_extension_of_the_format_when_it_is_missing() {
        let dir = scratch("export");
        let score = demo_score();

        let midi = export_score(&dir.join("song"), ExportFormat::Midi, &score).unwrap();
        assert_eq!(midi, dir.join("song.mid"));
        assert!(std::fs::read(&midi).unwrap().starts_with(b"MThd"));

        let xml = export_score(&dir.join("song.XML"), ExportFormat::MusicXml, &score).unwrap();
        assert_eq!(xml, dir.join("song.XML"), "an extension that fits is kept");
        assert!(std::fs::read_to_string(&xml)
            .unwrap()
            .contains("<score-partwise"));

        // A name that looks like another format is not used for this one.
        let odd = export_score(&dir.join("notes.txt"), ExportFormat::Midi, &score).unwrap();
        assert_eq!(odd, dir.join("notes.mid"));
    }

    #[test]
    fn a_failed_export_leaves_no_file_behind() {
        let dir = scratch("export-failure");
        let missing = dir.join("no-such-folder").join("song.mid");
        let error = export_score(&missing, ExportFormat::Midi, &demo_score()).unwrap_err();
        assert!(error.contains("Cannot export"));
        assert!(!missing.exists() && !dir.join("no-such-folder").exists());
    }

    #[test]
    fn exported_files_can_be_opened_again() {
        let dir = scratch("export-roundtrip");
        let score = demo_score();
        for (name, format) in [
            ("a.mid", ExportFormat::Midi),
            ("a.musicxml", ExportFormat::MusicXml),
        ] {
            let path = export_score(&dir.join(name), format, &score).unwrap();
            let opened = open_path(&path).unwrap();
            assert_eq!(
                opened.document.score().tracks.len(),
                score.tracks.len(),
                "{name}"
            );
            assert_eq!(opened.skipped_notes, 0, "{name}");
        }
    }

    #[test]
    fn imports_a_midi_file_and_names_it_after_the_file() {
        let (score, skipped) = import_midi_bytes(ONE_NOTE_MIDI, Some("my-song")).unwrap();
        assert_eq!(score.title, "my-song");
        assert_eq!(score.tracks.len(), 1);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn a_file_that_is_not_midi_gives_a_readable_error() {
        let error = import_midi_bytes(b"definitely not midi", None).unwrap_err();
        assert!(error.starts_with("invalid MIDI file"), "{error}");
    }

    #[test]
    fn a_midi_file_without_playable_notes_is_refused() {
        // Same file with the note moved to the percussion channel.
        let mut drums = ONE_NOTE_MIDI.to_vec();
        drums[23] = 0x99;
        drums[27] = 0x89;
        let error = import_midi_bytes(&drums, None).unwrap_err();
        assert!(error.contains("no notes"), "{error}");
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let error = open_path(Path::new("/nonexistent/song.mid")).err().unwrap();
        assert!(error.contains("/nonexistent/song.mid"), "{error}");
    }

    #[test]
    fn other_file_types_are_refused_with_what_is_supported() {
        let error = open_path(Path::new("/music/song.gp5")).err().unwrap();
        assert!(
            error.contains(".mpart") && error.contains(".mid"),
            "{error}"
        );
    }

    #[test]
    fn a_saved_score_opens_again_with_its_path_and_clean() {
        let path = scratch("save-open").join("song.mpart");
        save_score(&path, &demo_score()).unwrap();

        let opened = open_path(&path).unwrap();
        let state = opened.document.state();
        assert_eq!(state.score, demo_score());
        assert_eq!(state.path.as_deref(), path.to_str());
        assert!(!state.is_dirty);
    }

    #[test]
    fn a_damaged_score_file_says_what_is_wrong() {
        let path = scratch("damaged").join("broken.mpart");
        std::fs::write(&path, "{ not json").unwrap();
        let error = open_path(&path).err().unwrap();
        assert!(error.contains("not a Music Part"), "{error}");
    }

    #[test]
    fn a_saved_path_always_ends_in_mpart() {
        assert_eq!(score_path(Path::new("/m/song")), Path::new("/m/song.mpart"));
        assert_eq!(
            score_path(Path::new("/m/song.mid")),
            Path::new("/m/song.mpart")
        );
        assert_eq!(
            score_path(Path::new("/m/song.mpart")),
            Path::new("/m/song.mpart")
        );
    }

    #[test]
    fn the_file_from_the_command_line_must_exist_and_be_supported() {
        let dir = scratch("args");
        let midi = dir.join("SONG.MID");
        let text = dir.join("notes.txt");
        std::fs::write(&midi, ONE_NOTE_MIDI).unwrap();
        std::fs::write(&text, "x").unwrap();
        let (midi_arg, text_arg) = (midi.to_str().unwrap(), text.to_str().unwrap());

        assert_eq!(
            file_from_args(args(&["app", "--flag", midi_arg])),
            Some(midi.clone())
        );
        assert_eq!(file_from_args(args(&["app", text_arg])), None);
        assert_eq!(
            file_from_args(args(&["app", "/nonexistent/song.mid"])),
            None
        );
        assert_eq!(file_from_args(args(&["app"])), None);
    }
}
