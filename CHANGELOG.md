# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Playback: palm mutes, harmonics, hammer-ons, pull-offs and slide landings now have their own
  tone, not only a different volume: palm mutes are dull and stop short, harmonics are pure,
  legato notes swell in without the pick attack. It works with the recordings too.
- Playback with recordings: the nylon guitar, the electric guitar and the electric bass now play
  sampled instruments (CC0 banks from FreePats, fetched by `pnpm fetch:soundfont`) instead of the
  General MIDI sound. Notes a recording does not cover fall back to the General MIDI sound.
- A new instrument, the electric guitar, and readable instrument names in the menus.
- `render_wav` example: render a score or MIDI file to a WAV file to listen without the app.
- Export the score as a MIDI file or as MusicXML (tablature staff with string and fret, ties,
  voices and techniques), from the new Export menu. `io-midi` and `io-musicxml` can now write.
- `io-musicxml`: open MusicXML scores (`.musicxml`, `.xml`, compressed `.mxl`): parts, time
  signatures, tempo, tuning, tied notes, voices, and tablature strings and frets.
- Editing: select several beats (Shift+arrows or Shift+click), copy, cut and paste them, and
  empty a range with Delete.
- Editing: change the time signature of a measure (and the ones after it that share it), rename a
  track and change its instrument.
- Playback: click above a measure or double-click a beat to move playback there, `Home` to go back
  to the start, and mute and solo buttons on each track.
- Playback: techniques now sound. Hammer-ons and pull-offs are soft, slides glide to the next note,
  bends rise a whole tone, palm mutes are muted and short, harmonics ring at the harmonic pitch.
- `LICENSE`: PolyForm Noncommercial 1.0.0, and a `README.md`.
- `score-model`: score data model (score, track, measure, voice, beat, note), string/fret to MIDI
  pitch conversion, dotted durations and JSON serialization.
- `score-model`: tied notes (`Note::tied`), across barlines included. Scores saved before this
  field existed still load.
- `io-midi`: MIDI import to a score, with string and fret placement, chords, rests, tempo, time
  signature and measure splitting. Notes split by a barline become tied notes.
- `audio-engine`: sequencer turning a score into timed note events, honouring ties.
- `audio-engine`: SoundFont player (play, pause, stop) with one MIDI channel and instrument per
  track, and a `cpal` audio output.
- Editing: click a note or a string to select it, type a fret (two digits for 10 to 24), delete,
  change the duration and dots, insert and delete measures, change the tempo. A measure always
  stays exactly full. Undo and redo (200 steps). See `docs/editing.md`.
- Editing: ties (`L`) and techniques (hammer-on, pull-off, slide, bend, palm mute, harmonic),
  adding and removing tracks, and choosing the tuning of a track by note names.
- `score-model`: `Edit` variants `SetTied`, `SetTechnique`, `AddTrack`, `RemoveTrack`, `SetTuning`.
- Native score files (`.mpart`): New, Open, Save and Save as, with a versioned format and safe
  writing. The app asks before discarding unsaved changes, and shows them in the title.
- `score-model`: `Edit` and `apply_edit`, `Score::validate`, and `blank_score`.
- `io-native`: the `.mpart` format.
- `pnpm e2e`: end-to-end tests of the UI in headless Chrome against the real document code, and
  IPC tests of the commands through Tauri's mock runtime.
- Open MIDI files: an Open button (Ctrl+O), drag and drop onto the window, and a file given on the
  command line or through "Open with". The score is named after the file when it has no title.
- `io-midi`: each MIDI channel becomes its own track (so format 0 files are split), the instrument
  is chosen from the General MIDI program (bass, violin, guitar) with its own tuning, and
  chords are spread over distinct strings. Percussion is not imported. Notes no string can play
  are left out and reported in the app.
- `score-model`: standard tunings for bass and violin (`Instrument::standard_tuning`).
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

### Changed

- Playback is louder: the synthesizer gain went from 0.2 to 0.6.

### Fixed

- A score with more than 15 tracks no longer makes tracks share a MIDI channel (and so an
  instrument) during playback.
- Typing a fret on a very high string no longer builds a note above MIDI pitch 127.
- `io-midi`: the importer no longer panics or allocates without bound on unusual files: a zero
  time division, a time signature the grid cannot hold (such as 3/64), an extreme tempo, a huge
  gap between notes (refused as too long), and notes that are never released.
- `audio-engine`: the audio output now opens at the sample rate of the synthesizer. It used to
  open the device at its maximum rate, which would have played too fast and too high on a card
  that goes beyond the default rate.
