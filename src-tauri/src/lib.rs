//! Tauri shell: exposes the score and audio engines to the UI as commands and events.

mod audio;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};

use audio::AudioController;
use score_model::Score;

/// Overrides the bundled SoundFont, for development.
const SOUNDFONT_ENV: &str = "MUSIC_PART_SOUNDFONT";
const SOUNDFONT_RESOURCE: &str = "soundfont/GeneralUser-GS.sf2";
const POSITION_EVENT: &str = "playback-position";
const POSITION_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackPosition {
    /// Position in 1/128 of a whole note.
    tick: f64,
    is_playing: bool,
}

/// Largest MIDI file read. Real files are a few hundred kilobytes at most.
const MAX_MIDI_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedScore {
    score: Score,
    /// Notes the instrument could not play, left out of the score.
    skipped_notes: usize,
}

/// The MIDI file given on the command line (or by "Open with"), read once at startup.
struct InitialFile(Option<String>);

/// First argument that names an existing MIDI file. Flags and other arguments are ignored.
fn midi_file_from_args(args: impl Iterator<Item = String>) -> Option<String> {
    args.skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .find(|arg| {
            let path = Path::new(arg);
            let is_midi = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("mid") || e.eq_ignore_ascii_case("midi"));
            is_midi && path.is_file()
        })
}

#[tauri::command]
fn initial_file(initial: State<InitialFile>) -> Option<String> {
    initial.0.clone()
}

#[tauri::command]
fn demo_score() -> Score {
    score_model::demo_score()
}

#[tauri::command]
async fn open_midi(path: String) -> Result<OpenedScore, String> {
    let path = Path::new(&path);
    let bytes = read_midi_file(path)?;
    import_midi_bytes(&bytes, path.file_stem().and_then(|s| s.to_str()))
}

fn read_midi_file(path: &Path) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if size > MAX_MIDI_BYTES {
        return Err(format!("{} is too large to be a MIDI file", path.display()));
    }
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// `file_stem` names the score when the file does not carry a title.
fn import_midi_bytes(bytes: &[u8], file_stem: Option<&str>) -> Result<OpenedScore, String> {
    let mut imported = io_midi::import_midi(bytes).map_err(|e| e.to_string())?;
    if imported.score.tracks.is_empty() {
        return Err("This file has no notes that a string instrument can play.".into());
    }
    if let (Some(stem), "Untitled") = (file_stem, imported.score.title.as_str()) {
        imported.score.title = stem.to_string();
    }
    Ok(OpenedScore {
        score: imported.score,
        skipped_notes: imported.skipped_notes,
    })
}

#[tauri::command]
fn load_score(score: Score, audio: State<AudioController>) -> Result<(), String> {
    audio.load(score)
}

#[tauri::command]
fn play(audio: State<AudioController>) -> Result<(), String> {
    audio.play()
}

#[tauri::command]
fn pause(audio: State<AudioController>) -> Result<(), String> {
    audio.pause()
}

#[tauri::command]
fn stop(audio: State<AudioController>) -> Result<(), String> {
    audio.stop()
}

/// Tells the UI where playback is, about 30 times a second while it moves, and once more when it
/// stops so the UI never misses the final state.
fn spawn_position_emitter(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_playing = false;
        loop {
            std::thread::sleep(POSITION_INTERVAL);
            let Some(state) = app.state::<AudioController>().state() else {
                continue;
            };
            let is_playing = state.is_playing();
            if is_playing || last_playing {
                let position = PlaybackPosition {
                    tick: state.position_ticks(),
                    is_playing,
                };
                let _ = app.emit(POSITION_EVENT, position);
            }
            last_playing = is_playing;
        }
    });
}

fn soundfont_path(app: &tauri::App) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(SOUNDFONT_ENV) {
        return Ok(path.into());
    }
    app.path()
        .resolve(SOUNDFONT_RESOURCE, BaseDirectory::Resource)
        .map_err(|e| format!("cannot locate the bundled SoundFont: {e}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let audio = match soundfont_path(app) {
                Ok(path) => {
                    eprintln!("SoundFont: {}", path.display());
                    AudioController::start(path)
                }
                Err(reason) => {
                    eprintln!("Audio unavailable: {reason}");
                    AudioController::unavailable(reason)
                }
            };
            app.manage(audio);
            app.manage(InitialFile(midi_file_from_args(std::env::args())));
            spawn_position_emitter(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            demo_score,
            initial_file,
            open_midi,
            load_score,
            play,
            pause,
            stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Music Part application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_score_command_returns_the_demo() {
        assert_eq!(demo_score(), score_model::demo_score());
    }

    /// One quarter note (E4) at 480 ticks per quarter, written by hand.
    const ONE_NOTE_MIDI: &[u8] = &[
        0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0, // header
        0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 13, // track, 13 bytes
        0x00, 0x90, 0x40, 0x64, // note on E4
        0x83, 0x60, 0x80, 0x40, 0x00, // note off after 480 ticks
        0x00, 0xFF, 0x2F, 0x00, // end of track
    ];

    #[test]
    fn imports_a_midi_file_and_names_it_after_the_file() {
        let opened = import_midi_bytes(ONE_NOTE_MIDI, Some("my-song")).unwrap();
        assert_eq!(opened.score.title, "my-song");
        assert_eq!(opened.score.tracks.len(), 1);
        assert_eq!(opened.skipped_notes, 0);
    }

    #[test]
    fn a_file_that_is_not_midi_gives_a_readable_error() {
        let error = import_midi_bytes(b"definitely not midi", None).unwrap_err();
        assert!(error.starts_with("invalid MIDI file"), "{error}");
    }

    #[test]
    fn a_file_without_playable_notes_is_refused() {
        // Same file with the note moved to the percussion channel.
        let mut drums = ONE_NOTE_MIDI.to_vec();
        drums[23] = 0x99;
        drums[27] = 0x89;
        let error = import_midi_bytes(&drums, None).unwrap_err();
        assert!(error.contains("no notes"), "{error}");
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let error = read_midi_file(Path::new("/nonexistent/song.mid")).unwrap_err();
        assert!(error.contains("/nonexistent/song.mid"), "{error}");
    }

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn the_command_line_file_must_exist_and_be_midi() {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        // Cargo.toml exists but is not MIDI; the program name (first argument) is skipped.
        assert_eq!(midi_file_from_args(args(&["app", manifest])), None);
        assert_eq!(
            midi_file_from_args(args(&["app", "/nonexistent/song.mid"])),
            None
        );
        assert_eq!(midi_file_from_args(args(&["app"])), None);
    }

    #[test]
    fn a_midi_file_on_the_command_line_is_found_whatever_the_case() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data");
        std::fs::create_dir_all(dir).unwrap();
        let path = format!("{dir}/SONG.MID");
        std::fs::write(&path, ONE_NOTE_MIDI).unwrap();

        let found = midi_file_from_args(args(&["app", "--flag", &path]));
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(found.as_deref(), Some(path.as_str()));
    }
}
