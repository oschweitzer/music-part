//! Tauri shell: exposes the score and audio engines to the UI as commands.

use score_model::Score;

#[tauri::command]
fn demo_score() -> Score {
    score_model::demo_score()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![demo_score])
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
