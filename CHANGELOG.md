# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `score-model`: score data model (score, track, measure, voice, beat, note), string/fret to MIDI
  pitch conversion, dotted durations and JSON serialization.
- `score-model`: tied notes (`Note::tied`), across barlines included. Scores saved before this
  field existed still load.
- `io-midi`: MIDI import to a score, with string and fret placement, chords, rests, tempo, time
  signature and measure splitting. Notes split by a barline become tied notes.
- `audio-engine`: sequencer turning a score into timed note events, honouring ties.
- `audio-engine`: SoundFont player (play, pause, stop) with one MIDI channel and instrument per
  track, and a `cpal` audio output.
- `audio-engine`: `play_midi` example to listen to a MIDI file with a SoundFont.
- Desktop app (Tauri 2 + Svelte 5): window showing the guitar and bass tracks of a demo score as a
  tablature, with light and dark themes.
- UI: tablature rendering in SVG: strings per instrument, fret numbers, rests, stems, beams and
  flags, dotted notes, ties (with ghost frets across barlines), technique labels, time signature,
  automatic line breaking that adapts to the window width.
- `score-model`: `demo_score()`, a small two-track score used until import and editing reach the UI.
- CI on Linux, Windows and macOS: format, lint and tests for Rust, plus type check, tests and build
  for the UI.
