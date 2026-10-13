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
- CI on Linux, Windows and macOS: format, lint and tests.
