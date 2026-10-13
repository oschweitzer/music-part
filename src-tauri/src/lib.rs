//! Tauri shell: exposes the score and audio engines to the UI as commands and events.

mod audio;

use std::path::PathBuf;
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

#[tauri::command]
fn demo_score() -> Score {
    score_model::demo_score()
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
            spawn_position_emitter(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            demo_score, load_score, play, pause, stop
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
}
