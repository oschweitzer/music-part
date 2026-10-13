use std::fs;
use std::path::PathBuf;

use io_native::{read_file, write_file, NativeError};
use score_model::{blank_score, demo_score};

/// A fresh directory under `target/`, so tests never touch anything outside the repository.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("io-native")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_saved_score_reads_back_identical() {
    let path = scratch("roundtrip").join("song.mpart");
    write_file(&path, &demo_score()).unwrap();
    assert_eq!(read_file(&path).unwrap(), demo_score());
}

#[test]
fn saving_leaves_no_temporary_file_and_replaces_the_old_one() {
    let dir = scratch("replace");
    let path = dir.join("song.mpart");
    write_file(&path, &blank_score("first")).unwrap();
    write_file(&path, &blank_score("second")).unwrap();

    assert_eq!(read_file(&path).unwrap().title, "second");
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn a_failed_save_keeps_the_previous_file() {
    let dir = scratch("failed");
    let path = dir.join("song.mpart");
    write_file(&path, &blank_score("keep me")).unwrap();

    // A directory in the way of the temporary file makes the write fail.
    fs::create_dir(dir.join("song.mpart.saving")).unwrap();
    assert!(matches!(
        write_file(&path, &blank_score("lost")),
        Err(NativeError::Io(_))
    ));
    assert_eq!(read_file(&path).unwrap().title, "keep me");
}

#[test]
fn a_missing_file_is_an_io_error() {
    let path = scratch("missing").join("nope.mpart");
    assert!(matches!(read_file(&path), Err(NativeError::Io(_))));
}
