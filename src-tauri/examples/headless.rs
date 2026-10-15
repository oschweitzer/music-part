//! The document backend without a window or audio, for the end-to-end tests of the UI.
//!
//! It reads one JSON request per line on stdin and answers with one JSON line on stdout:
//! `{"id": 1, "cmd": "edit_score", "args": {...}}` gives `{"id": 1, "ok": ...}` or
//! `{"id": 1, "err": "message"}`. The commands and their JSON are those of the app, because both
//! call `service`. An optional first argument opens that file, as the app does. The sound packs
//! listed are those of the folder in `MUSIC_PART_PACKS`, as in the app; there is no audio, so no
//! sample file is read and none fails.

#[path = "../src/document.rs"]
#[allow(dead_code)]
mod document;
#[path = "../src/files.rs"]
#[allow(dead_code)]
mod files;
#[path = "../src/service.rs"]
mod service;

use std::io::{BufRead, Write};

use document::Document;
use serde_json::{json, Value};
use service::{FoundPacks, Startup};

fn string_arg(args: &Value, name: &str) -> Result<String, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing argument \"{name}\""))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn call(
    document: &mut Document,
    startup: &Startup,
    packs: &FoundPacks,
    cmd: &str,
    args: &Value,
) -> Result<Value, String> {
    match cmd {
        "startup" => to_value(service::startup(document, startup)),
        "new_document" => to_value(service::new_document(document)),
        "open_file" => to_value(service::open_file(document, &string_arg(args, "path")?)?),
        "save_file" => {
            let path = args.get("path").and_then(Value::as_str).map(str::to_owned);
            to_value(service::save_file(document, path)?)
        }
        "export_file" => {
            let format = serde_json::from_value(args.get("format").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("invalid format: {e}"))?;
            to_value(service::export_file(
                document,
                &string_arg(args, "path")?,
                format,
            )?)
        }
        "edit_score" => {
            let edit = serde_json::from_value(args.get("edit").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("invalid edit: {e}"))?;
            to_value(service::edit_score(document, &edit)?)
        }
        "undo" => to_value(service::undo(document)),
        "redo" => to_value(service::redo(document)),
        "sound_packs" => to_value(service::sound_packs(packs, &[])),
        other => Err(format!("unknown command \"{other}\"")),
    }
}

fn main() {
    let (mut document, startup) = service::initial_document(std::env::args());
    let roots: Vec<std::path::PathBuf> = std::env::var_os("MUSIC_PART_PACKS")
        .map(Into::into)
        .into_iter()
        .collect();
    let packs = FoundPacks::discover(&roots, None);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let id = request["id"].clone();
        let cmd = request["cmd"].as_str().unwrap_or_default();
        let args = request.get("args").cloned().unwrap_or(Value::Null);
        let answer = match call(&mut document, &startup, &packs, cmd, &args) {
            Ok(ok) => json!({ "id": id, "ok": ok }),
            Err(err) => json!({ "id": id, "err": err }),
        };
        let _ = writeln!(stdout, "{answer}");
        let _ = stdout.flush();
    }
}
