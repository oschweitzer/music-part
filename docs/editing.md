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
| `Delete` / `Backspace`      | Removes the note on the selected string (a beat left empty becomes a rest)    |
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

`+ Track` adds a guitar, bass or violin in standard tuning with empty measures matching the first
track. `Remove track` deletes the current one (the last track cannot be removed). The tuning field
takes note names from the highest string to the lowest, for example `E4 B3 G3 D3 A2 D2` (`#` and `b`
are accepted, middle C is `C4`); the number of strings follows the number of names, up to 12. A
tuning that would leave a note without a string, or above MIDI pitch 127, is refused.

Edits are recorded one by one, up to 200 steps. An edit that changes nothing is not recorded. Undoing
back to the state that was opened or saved clears the unsaved mark.

## Files

- **`.mpart`** is the native format: the score as JSON in an envelope that names the format and its
  version. Saving writes to a temporary file first and renames it, so a failed save never damages an
  existing file. A file saved by a newer version of the app is refused with a clear message.
- **`.mid`** files can be opened, not saved. Opening one starts a new, unsaved score.
- The app asks before discarding unsaved changes: when opening a file, creating a score, or closing
  the window.
- A file given on the command line, or dropped on the window, is opened.

## Not there yet

There is no way to change the time signature, rename a track or change its instrument, or select
several notes and copy and paste.
