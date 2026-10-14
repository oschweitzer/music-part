# Writing and editing

The desktop app edits one score at a time. The score lives in the Rust side (`src-tauri/src/document.rs`);
the UI sends edits and shows what comes back.

## Selecting

Click a note, or an empty spot on a string, to select a beat and a string. The selected spot is
outlined, and its beat is lightly highlighted. The arrow keys move it: left and right along the beats
(crossing barlines), up and down between strings.

## Keyboard

| Key                         | Does                                                                          |
| --------------------------- | ----------------------------------------------------------------------------- |
| `0`-`9`                     | Sets the fret on the selected string. Two digits typed quickly make one fret  |
| `Delete` / `Backspace`      | Removes the note on the selected string (a beat left empty becomes a rest).   |
|                             | With several beats selected, empties all of them                              |
| `Shift+Left` / `Shift+Right`| Selects several beats                                                         |
| `Ctrl+C` `Ctrl+X` `Ctrl+V`  | Copy, cut and paste the selected beats                                        |
| `W` `H` `Q` `E` `S` `T`     | Whole, half, quarter, eighth, sixteenth and thirty-second note                |
| `.`                         | Adds or removes a dot                                                         |
| `L`                         | Ties the note to the one before it on the same string (again: unties)         |
| `B` `M` `/`                 | Bend, palm mute, slide on the note (again: clears). Others are in the toolbar |
| `Ctrl+Enter`                | Inserts a measure after the selected one                                      |
| `Ctrl+Z`                    | Undo                                                                          |
| `Ctrl+Shift+Z` / `Ctrl+Y`   | Redo                                                                          |
| `Ctrl+N` `Ctrl+O` `Ctrl+S`  | New, open, save. `Ctrl+Shift+S` saves under a new name                        |
| `Space`                     | Play or pause                                                                 |
| `Home`                      | Moves playback back to the beginning                                          |
| `Esc`                       | Clears the selection                                                          |

On macOS, `Cmd` replaces `Ctrl`. The same actions are in the toolbars.

## Copy and paste

`Shift+Left` and `Shift+Right` (or Shift+click) select the beats from where you started to the beat
you reach, across barlines. Copy keeps those beats of the current track, with all their strings,
their durations and their techniques. Paste writes them one after the other from the selected beat
(the first of the range, when several are selected), over what is there:

- A pasted beat that crosses a barline is cut, and the part in the next measure is tied on.
- The measures stay full: what the paste shortens becomes rests.
- Notes on strings the track does not have (a guitar chord pasted on a bass) are left out.
- A paste that would run past the end of the score is refused. Add measures first.
- Cut is a copy followed by emptying the range: the rhythm stays, as rests.

The copy lives in the app and is lost when it closes; it is not on the system clipboard.

## Playing from somewhere else

Click the strip just above a measure (where its number is) to move playback to the start of that
measure, or double-click a beat to move it there. This works while playing or paused. A note that is
already sounding at the new position is not started: playback picks up with the next one.

Each track tab has `M` (mute) and `S` (solo) buttons. A track plays unless it is muted, but as soon
as any track is soloed only the soloed ones play. Mute and solo only change what you hear; they are
not saved in the score, and they are forgotten when the app closes. Editing the score stops
playback but keeps them.

## Rules

A measure is always exactly full. Changing the length of a beat keeps it that way:

- **Shorter**: the freed time becomes rests right after the beat.
- **Longer**: the beats that follow are overwritten, notes included. If the measure has no room left
  after the beat, nothing changes and the app says so.
- A fret is 0 to 24. A dotted thirty-second note is not available.
- A tie copies the fret of the note it continues, and needs a note on the same string just before
  (across a barline too). Retyping the fret of a tied note unties it.
- Ties and techniques apply to a note; on a rest the app asks for a note first.
- Inserting or deleting a measure applies to every track, so tracks always have the same length. The
  last measure cannot be deleted.

## How techniques sound

The player turns techniques into MIDI (`audio-engine/src/sequencer.rs`):

| Technique           | Sound                                                                            |
| ------------------- | -------------------------------------------------------------------------------- |
| Hammer-on, pull-off | The note is played softly, without the attack of a pick                          |
| Slide               | Glides, in the second half of the note, to the note on the same string that      |
|                     | follows (up to an octave away), which is then played softly. Nothing to reach:   |
|                     | a plain note                                                                     |
| Bend                | Rises a whole tone over the start of the note and is held until it ends          |
| Palm mute           | Quieter, and cut to half its length                                              |
| Harmonic            | Natural harmonic: frets 12 and 24 an octave above the open string, 7 and 19 an   |
|                     | octave and a fifth, 5 two octaves, 4 two octaves and a third, 9 an octave and a  |
|                     | major third; any other fret an octave above the fretted note                     |

Bends and slides move the pitch wheel of the track's MIDI channel, so they bend every note sounding
on that track at the same time. A bend always rises a whole tone: the model has no bend amount yet.

## Tracks and tuning

`+ Track` adds a nylon guitar, electric guitar, electric bass or violin in standard tuning with empty measures matching the first
track. `Remove track` deletes the current one (the last track cannot be removed). The tuning field
takes note names from the highest string to the lowest, for example `E4 B3 G3 D3 A2 D2` (`#` and `b`
are accepted, middle C is `C4`); the number of strings follows the number of names, up to 12. A
tuning that would leave a note without a string, or above MIDI pitch 127, is refused.

The track bar also has the track's name and its sound (nylon guitar, electric guitar, electric bass or violin). Changing the sound
keeps the strings: retune the track if they differ.

## Time signature

The two fields on the toolbar show the time signature of the selected measure. Changing one applies
to that measure and to the measures after it that had the same signature, in every track. In a
shorter measure the notes that no longer fit are cut: a note that crosses the new end is shortened,
and when its new length needs two durations the second part is tied on. A longer measure is filled
with rests. The denominator is 1, 2, 4, 8, 16 or 32, and a measure must be a whole number of 32nd
notes.

Edits are recorded one by one, up to 200 steps. An edit that changes nothing is not recorded. Undoing
back to the state that was opened or saved clears the unsaved mark.

## Files

- **`.mpart`** is the native format: the score as JSON in an envelope that names the format and its
  version. Saving writes to a temporary file first and renames it, so a failed save never damages an
  existing file. A file saved by a newer version of the app is refused with a clear message.
- **`.mid`** and **MusicXML** (`.musicxml`, `.xml`, `.mxl`) files can be opened, not saved. Opening
  one starts a new, unsaved score. See [`musicxml.md`](musicxml.md) for what is read.
- **Export** (toolbar menu) writes the score as a MIDI file or as MusicXML. It does not save the
  score or change its name, and the unsaved mark stays as it was. The extension is added when the
  dialog leaves it off.
  - MIDI: one track per instrument on its own channel, tempo and time signatures, ties merged
    into long notes. Techniques are not written.
  - MusicXML: one tablature staff per track with its tuning, string and fret on every note, ties,
    voices, and the techniques (hammer-on, pull-off, slide, bend, harmonic, palm mute). Other
    programs can show and play it, and Music Part reads it back to the same score.
- The app asks before discarding unsaved changes: when opening a file, creating a score, or closing
  the window.
- A file given on the command line, or dropped on the window, is opened.

## Not there yet

There is no system clipboard, no select-all, and no copy between tracks of different voices (only the
first voice of a measure is edited).
