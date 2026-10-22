# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Playback with recordings: the bass played with a pick (FreePats Picked Bass YR) and the banjo
  (ganjo, a six-string banjo from sfzinstruments, the first bundled SFZ pack) now play recorded
  samples, both CC0. The sounds still without a free recording are listed in
  `docs/soundfonts.md`.

- Sound packs can declare new sounds of an existing instrument (a twelve-string guitar as
  `"guitar/twelve-string"`), with a name and the General MIDI program that stands in for them.
  The Sound and `+ Track` menus list them after the built-in sounds, and the `Sounds` panel marks
  them as new. Drum kits stay the built-in ones. See `docs/sound-packs.md`.
- `.mpart`: a track with a pack's sound records that program as `fallback_program`, so a score
  opened where the pack is missing still loads and plays it, and the Sound menu shows the sound as
  "(pack missing)". MIDI and MusicXML exports write that program. Older files load unchanged and
  files with built-in sounds are written as before.

- Drums: a new instrument whose notes are the pieces of a kit (General MIDI percussion keys 35 to
  81), written on a percussion staff (x heads for cymbals and hi-hats, stems up for the hands and
  down for the feet) and played on MIDI channel 10. Six kits: standard, room, power, electronic,
  jazz and brush. A drum note is saved as `{"piece":38,...}`; older files load unchanged. See
  `docs/drums.md`.
- UI: writing drums: digits `1` to `0` strike the pieces of a palette shown in the toolbar for a
  drum track (kick, snare, closed, open and pedal hi-hat, crash, ride, toms), which also lists
  every other piece by name; double-click a line to strike its drum; `Up` and `Down` move a hit
  to the next line of the staff.
- `io-midi`: the percussion channel (10) is imported as a drum track, its program picking the kit,
  and drum tracks are exported on it.
- `io-musicxml`: parts of `unpitched` notes are imported as drum tracks (the piece from the
  note's `instrument` and `midi-unpitched`, else from its place and head), and drum tracks are
  exported with a percussion clef, one instrument per piece and `unpitched` notes.
- Sound packs can give a sound an SFZ instrument (`.sfz` and WAV samples) instead of a SoundFont,
  played by a small SFZ player in `audio-engine`: key and velocity ranges, round-robin, loops,
  amplitude envelope, volume, pan and tuning. Unsupported opcodes are ignored and reported on
  the console, sample paths must stay inside the pack, and a pack's samples are capped at
  256 MiB. Notes the instrument does not cover play the General MIDI sound. See `docs/sfz.md`.
- UI: a `Sounds` button in the toolbar lists the sound packs in use with their author and
  licence, the packs that could not be used and why, which pack (or General MIDI) plays the sound
  of the selected track, and the folder where the user adds packs.
- Sound packs: a pack's samples are read only when a track of the score uses one of its sounds,
  and freed when no track uses them any more, so unused packs cost no memory or start-up time.
- `score-model`: notes stored as pitches. A note has a place, a string and a fret
  (`Place::Fretted`) or a MIDI pitch (`Place::Pitched`), and the instrument says which
  (`NoteKind`). Files keep the same format: fretted notes are written as before and older
  `.mpart` files load unchanged; a pitched note is written `{"pitch":60,...}`. See
  `docs/notation.md`.
- A new instrument, the piano (sound `piano/grand`, General MIDI program 0), written on a grand
  staff. `Score::validate` checks its notes (pitches 0 to 127, no pitch twice in a beat).
- New instruments: the organ (drawbar, rock, church and reed sounds, General MIDI 16 to 20) on a
  grand staff, the synthesizer (square and saw leads, warm and choir pads, 80, 81, 89, 91) on a
  treble staff, and the voice (choir aahs, voice oohs, synth voice, 52 to 54). The piano gets a
  bright, an electric and an FM electric piano sound (1, 4, 5). See `docs/sounds.md`.
- A voice track has a clef: treble, treble an octave down (tenor) or bass, chosen in the new
  `Clef` menu (`Edit::SetClef`) and stored in the track (`clef`, left out for other instruments,
  so older files load unchanged). A track that becomes a voice, and a voice read from MIDI, get
  the clef of their range; MusicXML keeps the clef. See `docs/notation.md`.
- MIDI and MusicXML import map organ, synthesizer, voice and other piano programs to these
  sounds instead of the grand piano, and MusicXML part names such as "Organ", "Synth", "Vocals",
  "Soprano", "Tenor" or "Bass voice" too ("Bass" alone stays the bass guitar). Exports write
  their programs.
- `score-model`: edits by pitch, `AddPitch`, `RemovePitch`, `MovePitch`, `SetPitchTied`, and a
  `pitches` span on `ClearNotes`; on a string track a pitch goes on a free string.
- UI: a standard notation staff, with clefs, key signatures, accidentals, ledger lines, stems,
  beams, rests, dots and ties, in the tabs and the rows views. The new `Tab` / `Staff` / `Both`
  switch shows string tracks as tablature, on a staff, or both (the choice is remembered).
- UI: writing on a staff: double-click a line or space, or press `Enter` at the selected pitch or
  `Shift+A` to `Shift+G` for a note name; `Up` and `Down` move a note a semitone.
- UI: a rows view of the tracks, one under the other on the same time axis like a DAW, switched
  with the new `Tabs` / `Rows` toggle in the toolbar (the choice is remembered). Each row has a
  header with mute, solo, remove and fold, rows share their barlines and one playback cursor, and
  only what is on screen is drawn, so scores with many tracks stay readable.
- UI: `Alt+Up` and `Alt+Down` move the selection to the previous or next track.
- UI: a track menu next to the track fields picks the track to edit, and the track tabs scroll
  sideways when there are too many to fit.
- Sounds: a track now picks a sound of its instrument, from a Sound menu next to the Instrument
  menu: steel or nylon strings for the acoustic guitar; clean, muted, overdrive or distortion for
  the electric guitar; finger, pick or slap for the bass. The "+ Track" menu lists every sound. See
  `docs/sounds.md`.
- A new instrument, the five-string banjo (open G tuning), whose short fifth string is numbered
  from the neck like in banjo tablature: it has no frets 1 to 5.
- `score-model`: `Sound`, the sounds table (id such as `"electric-guitar/distortion"`, name,
  General MIDI program, instrument), `Track::sound`, the `SetSound` edit and an optional `sound`
  on `AddTrack`. Scores saved before sounds existed load with the sound they had.
- Sound packs: a folder with a `pack.json` manifest (name, author, licence, sounds and their key
  range) and SoundFont samples gives recordings to sounds. The app loads the bundled packs and
  the user's own from `packs/` in its data folder, a user's pack replacing a bundled one with the
  same id. See `docs/sound-packs.md`.
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

- UI: the track bar is now two lines. The first holds the track tabs and `+ Track`; the second
  holds the settings of the selected track (name, instrument, sound, clef, tuning) and starts with
  that track's name. In the Rows view there are no tabs, so `+ Track` sits at the end of the second
  line. The `Track to edit` menu is gone: click a tab or a row header instead.
- UI: in the Tabs view the tablature now uses the whole width of the window instead of stopping at
  1100 px, with a narrower margin around it.
- Sound packs: a pack that lists a drum kit is refused; drums always play the SoundFont's kits.
- Opening a MIDI file that only has drums now opens it instead of saying it has no notes.
- Import: MIDI and MusicXML parts that are not a string instrument (keyboards, winds, voices...)
  now become piano tracks that keep every note, instead of nylon guitar tracks that lost the notes
  no string could play. A MusicXML piano part is merged into one voice, held notes tied on.
- Export: piano tracks are written to MIDI by pitch and to MusicXML on a standard staff.
- Changing the instrument of a track between a string instrument and the piano converts its
  notes; it is refused when a pitch cannot be placed on the strings.
- The three recorded sounds (nylon guitar, clean electric guitar, finger bass) are now sound
  packs in `assets/soundfonts/packs/`, each with its licence text. `MUSIC_PART_BANKS` is replaced
  by `MUSIC_PART_PACKS`, and `render_wav --no-banks` by `--no-packs`.

- `io-midi` and `io-musicxml`: a General MIDI program now gives the closest sound, not only an
  instrument (program 30 is a distorted electric guitar, 25 a steel-string guitar, 105 a banjo),
  and exports write the program of the track's sound.
- Playback: the sound of a track chooses its General MIDI program, and a sample bank now plays one
  sound (the nylon guitar, the clean electric guitar, the finger bass) instead of every sound of
  its instrument.
- Changing the instrument of a track gives it the default sound of the new instrument. The former
  "Nylon guitar" is now the acoustic guitar, with nylon strings as its default sound.
- Playback is louder: the synthesizer gain went from 0.2 to 0.6.
- A theme button in the toolbar goes round the system theme, the light theme and the dark theme;
  the choice is remembered.
- Editing: drag the mouse to select a rectangle of notes, beats by strings, so part of a chord can
  be selected, deleted, copied or cut. `score-model`: `ClearNotes` edit.
- Editing: `Delete` on a rest (or on a selection of rests) takes it out of the timeline in every
  track, shortening the measure, and removes the measure when its last rest goes. Refused, with a
  message, while another track has a note there. `score-model`: `RemoveRests` edit.
- Editing: right-click the tablature for a menu (cut, copy, paste, insert a measure before or
  after, delete measures), and `Ctrl+Delete` to delete measures. A range of beats deletes every
  measure it touches, in one undo step. `score-model`: `DeleteMeasure` takes a `count`.
- Editing: drag the mouse over several beats to select them, then Delete, Ctrl+C or Ctrl+X.
- Each track tab has a cross to remove it, replacing the "Remove track" button at the far end of
  the track bar, which was easy to miss.
- The tempo field is wider, so a three-digit tempo is fully visible.

### Fixed

- Playback: with the sample banks loaded, the guitars played the bass recording. Each instrument
  now plays its own bank.
- A score with more than 15 tracks no longer makes tracks share a MIDI channel (and so an
  instrument) during playback.
- Typing a fret on a very high string no longer builds a note above MIDI pitch 127.
- `io-midi`: the importer no longer panics or allocates without bound on unusual files: a zero
  time division, a time signature the grid cannot hold (such as 3/64), an extreme tempo, a huge
  gap between notes (refused as too long), and notes that are never released.
- `audio-engine`: the audio output now opens at the sample rate of the synthesizer. It used to
  open the device at its maximum rate, which would have played too fast and too high on a card
  that goes beyond the default rate.
