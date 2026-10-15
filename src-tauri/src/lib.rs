//! Tauri shell: exposes the document (score, edits, files) and the audio engine to the UI as
//! commands and events. The score lives here; the UI shows what it is sent.

mod audio;
mod document;
mod files;
mod service;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State};

use audio::AudioController;
use document::{Document, DocumentState};
use score_model::Edit;
use service::{OpenedDocument, Startup};

/// Overrides the bundled SoundFont, for development.
const SOUNDFONT_ENV: &str = "MUSIC_PART_SOUNDFONT";
const SOUNDFONT_RESOURCE: &str = "soundfont/GeneralUser-GS.sf2";
/// Overrides the folder of bundled sound packs, for development.
const PACKS_ENV: &str = "MUSIC_PART_PACKS";
const PACKS_RESOURCE: &str = "soundfont/packs";
/// Folder of the user's own sound packs, in the app data directory.
const USER_PACKS_DIR: &str = "packs";
const POSITION_EVENT: &str = "playback-position";
const POSITION_INTERVAL: Duration = Duration::from_millis(33);

type SharedDocument = Mutex<Document>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackPosition {
    /// Position in 1/128 of a whole note.
    tick: f64,
    is_playing: bool,
}

/// A panic while the document was locked must not make every later command fail.
fn locked(document: &SharedDocument) -> MutexGuard<'_, Document> {
    document
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The audio engine plays whatever the document holds. If audio is unavailable the user finds out
/// when pressing play, so a failure here is not worth interrupting an edit for.
fn sync_audio(document: &Document, audio: &AudioController) {
    let _ = audio.load(document.score().clone());
}

#[tauri::command]
fn startup(document: State<SharedDocument>, startup: State<Startup>) -> OpenedDocument {
    service::startup(&locked(&document), &startup)
}

#[tauri::command]
fn new_document(document: State<SharedDocument>, audio: State<AudioController>) -> DocumentState {
    let mut document = locked(&document);
    let state = service::new_document(&mut document);
    sync_audio(&document, &audio);
    state
}

#[tauri::command]
async fn open_file(
    path: String,
    document: State<'_, SharedDocument>,
    audio: State<'_, AudioController>,
) -> Result<OpenedDocument, String> {
    let mut document = locked(&document);
    let opened = service::open_file(&mut document, &path)?;
    sync_audio(&document, &audio);
    Ok(opened)
}

#[tauri::command]
fn save_file(
    path: Option<String>,
    document: State<SharedDocument>,
) -> Result<DocumentState, String> {
    service::save_file(&mut locked(&document), path)
}

#[tauri::command]
fn export_file(
    path: String,
    format: files::ExportFormat,
    document: State<SharedDocument>,
) -> Result<String, String> {
    service::export_file(&locked(&document), &path, format)
}

#[tauri::command]
fn edit_score(
    edit: Edit,
    document: State<SharedDocument>,
    audio: State<AudioController>,
) -> Result<DocumentState, String> {
    let mut document = locked(&document);
    let state = service::edit_score(&mut document, &edit)?;
    sync_audio(&document, &audio);
    Ok(state)
}

#[tauri::command]
fn undo(document: State<SharedDocument>, audio: State<AudioController>) -> DocumentState {
    let mut document = locked(&document);
    let state = service::undo(&mut document);
    sync_audio(&document, &audio);
    state
}

#[tauri::command]
fn redo(document: State<SharedDocument>, audio: State<AudioController>) -> DocumentState {
    let mut document = locked(&document);
    let state = service::redo(&mut document);
    sync_audio(&document, &audio);
    state
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

#[tauri::command]
fn seek(tick: f64, audio: State<AudioController>) -> Result<(), String> {
    audio.seek(tick)
}

/// `audible[i]` says whether track `i` makes sound; the UI works out mute and solo.
#[tauri::command]
fn set_audible(audible: Vec<bool>, audio: State<AudioController>) -> Result<(), String> {
    audio.set_audible(audible)
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

/// The folders of sound packs, lowest priority first: the bundled packs, then the user's, so a
/// user's pack replaces a bundled one with the same id. A folder that does not exist is skipped.
fn pack_roots(app: &tauri::App) -> Vec<PathBuf> {
    let bundled = match std::env::var_os(PACKS_ENV) {
        Some(path) => Some(path.into()),
        None => app
            .path()
            .resolve(PACKS_RESOURCE, BaseDirectory::Resource)
            .ok(),
    };
    let user = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(USER_PACKS_DIR));
    bundled.into_iter().chain(user).collect()
}

fn soundfont_path(app: &tauri::App) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(SOUNDFONT_ENV) {
        return Ok(path.into());
    }
    app.path()
        .resolve(SOUNDFONT_RESOURCE, BaseDirectory::Resource)
        .map_err(|e| format!("cannot locate the bundled SoundFont: {e}"))
}

/// Registers every command. The app and the tests build on this, so the tests exercise the same
/// command names, argument names and JSON the UI really uses.
fn with_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        startup,
        new_document,
        open_file,
        save_file,
        export_file,
        edit_score,
        undo,
        redo,
        play,
        pause,
        stop,
        seek,
        set_audible
    ])
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let audio = match soundfont_path(app) {
                Ok(path) => {
                    eprintln!("SoundFont: {}", path.display());
                    AudioController::start(path, pack_roots(app))
                }
                Err(reason) => {
                    eprintln!("Audio unavailable: {reason}");
                    AudioController::unavailable(reason)
                }
            };

            let (document, startup) = service::initial_document(std::env::args());
            sync_audio(&document, &audio);
            app.manage(audio);
            app.manage(Mutex::new(document));
            app.manage(startup);
            spawn_position_emitter(app.handle().clone());
            Ok(())
        });

    with_commands(builder)
        .run(context())
        .expect("error while running the Music Part application");
}

/// The app's configuration and assets. The macro defines a symbol on macOS, so it may only be
/// expanded once: the tests use this too.
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewWindow, WebviewWindowBuilder};

    /// An app with the real commands, configuration and permissions, and the demo score, but no
    /// window system and no audio.
    fn app() -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
        let app = with_commands(mock_builder()).build(context()).unwrap();
        app.manage(AudioController::unavailable("no audio in tests".into()));
        app.manage(Mutex::new(Document::new(score_model::demo_score(), None)));
        app.manage(Startup {
            skipped_notes: 0,
            notice: None,
        });
        let window = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        (app, window)
    }

    /// What the UI does: invoke a command with a JSON object of arguments.
    fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // The origin of the app itself, as for a request coming from the real UI.
            url: window.url().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(window, request).map(|body| body.deserialize::<Value>().unwrap())
    }

    #[test]
    fn startup_gives_the_demo_with_the_camel_case_keys_the_ui_reads() {
        let (_app, window) = app();
        let opened = invoke(&window, "startup", json!({})).unwrap();

        assert_eq!(opened["document"]["score"]["title"], "Demo in A minor");
        assert_eq!(opened["document"]["isDirty"], false);
        assert_eq!(opened["skippedNotes"], 0);
    }

    #[test]
    fn an_edit_sent_the_way_the_ui_sends_it_changes_the_score_and_the_history() {
        let (_app, window) = app();
        let edit = json!({ "type": "setFret", "track": 0, "measure": 0, "beat": 0, "string": 1, "fret": 7 });
        let state = invoke(&window, "edit_score", json!({ "edit": edit })).unwrap();

        let note = &state["score"]["tracks"][0]["measures"][0]["voices"][0]["beats"][0]["notes"][0];
        assert_eq!(
            (note["string"].as_u64(), note["fret"].as_u64()),
            (Some(1), Some(7))
        );
        assert_eq!(
            (state["isDirty"].as_bool(), state["canUndo"].as_bool()),
            (Some(true), Some(true))
        );

        let undone = invoke(&window, "undo", json!({})).unwrap();
        assert_eq!(undone["isDirty"], false);
        let redone = invoke(&window, "redo", json!({})).unwrap();
        assert_eq!(redone["isDirty"], true);
    }

    #[test]
    fn a_refused_edit_comes_back_as_the_message_the_ui_shows() {
        let (_app, window) = app();
        let edit = json!({ "type": "setFret", "track": 0, "measure": 0, "beat": 0, "string": 1, "fret": 99 });
        let error = invoke(&window, "edit_score", json!({ "edit": edit })).unwrap_err();
        assert_eq!(error, json!("the fret must be between 0 and 24"));
    }

    #[test]
    fn the_sound_of_a_track_is_sent_and_read_with_the_ids_of_the_ui() {
        let (_app, window) = app();
        let opened = invoke(&window, "startup", json!({})).unwrap();
        assert_eq!(
            opened["document"]["score"]["tracks"][1]["sound"],
            "bass/finger"
        );

        let edit = json!({ "type": "setSound", "track": 1, "sound": "bass/slap" });
        let state = invoke(&window, "edit_score", json!({ "edit": edit })).unwrap();
        assert_eq!(state["score"]["tracks"][1]["sound"], "bass/slap");

        let add = json!({ "type": "addTrack", "instrument": "ElectricGuitar", "sound": "electric-guitar/distortion" });
        let state = invoke(&window, "edit_score", json!({ "edit": add })).unwrap();
        assert_eq!(
            state["score"]["tracks"][2]["sound"],
            "electric-guitar/distortion"
        );

        let wrong = json!({ "type": "setSound", "track": 1, "sound": "violin/standard" });
        let error = invoke(&window, "edit_score", json!({ "edit": wrong })).unwrap_err();
        assert_eq!(error, json!("this sound belongs to another instrument"));
    }

    #[test]
    fn a_malformed_edit_is_an_error_and_not_a_crash() {
        let (_app, window) = app();
        assert!(invoke(
            &window,
            "edit_score",
            json!({ "edit": { "type": "teleport" } })
        )
        .is_err());
        assert!(invoke(&window, "edit_score", json!({})).is_err());
    }

    #[test]
    fn saving_and_opening_use_the_argument_names_of_the_ui() {
        let (_app, window) = app();
        // No file yet, and the UI passes null: the app must ask for one.
        let error = invoke(&window, "save_file", json!({ "path": null })).unwrap_err();
        assert!(
            error.as_str().unwrap().contains("choose where to save"),
            "{error}"
        );

        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/tauri-tests/ipc");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song").to_string_lossy().into_owned();
        let saved = invoke(&window, "save_file", json!({ "path": path })).unwrap();
        assert_eq!(saved["path"], json!(format!("{path}.mpart")));

        let opened = invoke(
            &window,
            "open_file",
            json!({ "path": format!("{path}.mpart") }),
        )
        .unwrap();
        assert_eq!(opened["document"]["isDirty"], false);

        let error = invoke(
            &window,
            "open_file",
            json!({ "path": "/nonexistent/x.mpart" }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("/nonexistent/x.mpart"),
            "{error}"
        );
    }

    #[test]
    fn a_new_document_is_blank_and_clean() {
        let (_app, window) = app();
        let state = invoke(&window, "new_document", json!({})).unwrap();
        assert_eq!(state["score"]["title"], "Untitled");
        assert_eq!(state["isDirty"], false);
    }

    #[test]
    fn playing_without_audio_reports_why_instead_of_failing_silently() {
        let (_app, window) = app();
        let error = invoke(&window, "play", json!({})).unwrap_err();
        assert_eq!(error, json!("no audio in tests"));
    }

    #[test]
    fn seeking_and_mixing_without_audio_report_why() {
        let (_app, window) = app();
        let error = invoke(&window, "seek", json!({ "tick": 64.0 })).unwrap_err();
        assert_eq!(error, json!("no audio in tests"));
        let error =
            invoke(&window, "set_audible", json!({ "audible": [true, false] })).unwrap_err();
        assert_eq!(error, json!("no audio in tests"));
    }

    #[test]
    fn exporting_writes_the_file_and_leaves_the_document_alone() {
        let (_app, window) = app();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/tauri-tests/ipc-export");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let written = invoke(
            &window,
            "export_file",
            json!({ "path": dir.join("song").to_str().unwrap(), "format": "midi" }),
        )
        .unwrap();
        assert!(written.as_str().unwrap().ends_with("song.mid"));
        assert!(std::fs::read(dir.join("song.mid"))
            .unwrap()
            .starts_with(b"MThd"));

        let error = invoke(
            &window,
            "export_file",
            json!({ "path": "/x", "format": "pdf" }),
        )
        .unwrap_err();
        assert!(error.to_string().contains("pdf"), "{error}");

        let state = invoke(&window, "undo", json!({})).unwrap();
        assert_eq!(
            state["path"],
            Value::Null,
            "exporting does not give the score a file"
        );
        assert_eq!(state["isDirty"], false);
    }
}
