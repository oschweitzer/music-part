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
- Playback in the app: play, pause and stop buttons (Space toggles), a clock, a cursor and a
  highlighted measure that follow the score, and automatic scrolling to the line being played.
  Playback uses the bundled GeneralUser GS SoundFont. In a plain browser the cursor still moves,
  silently, to develop the UI.
- `audio-engine`: `Player::load` swaps a prebuilt `Program` without reloading the SoundFont, and
  `Player::position_ticks` gives the position on the score. The player rewinds by itself at the end.
- `pnpm fetch:soundfont`: downloads the GeneralUser GS SoundFont with a pinned checksum. See
  `docs/soundfonts.md` for its licence and the provenance caveat.
- `audio-engine`: `play_midi` example to listen to a MIDI file with a SoundFont.
- Desktop app (Tauri 2 + Svelte 5): window showing the guitar and bass tracks of a demo score as a
  tablature, with light and dark themes.
- UI: tablature rendering in SVG: strings per instrument, fret numbers, rests, stems, beams and
  flags, dotted notes, ties (with ghost frets across barlines), technique labels, time signature,
  automatic line breaking that adapts to the window width.
- `score-model`: `demo_score()`, a small two-track score used until import and editing reach the UI.
- CI on Linux, Windows and macOS: format, lint and tests for Rust, plus type check, tests and build
  for the UI.

### Fixed

- `audio-engine`: the audio output now opens at the sample rate of the synthesizer. It used to
  open the device at its maximum rate, which would have played too fast and too high on a card
  that goes beyond the default rate.
