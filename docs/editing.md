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
| `Ctrl+Enter`                | Inserts a measure after the selected one                                      |
| `Ctrl+Z`                    | Undo                                                                          |
| `Ctrl+Shift+Z` / `Ctrl+Y`   | Redo                                                                          |
| `Ctrl+N` `Ctrl+O` `Ctrl+S`  | New, open, save. `Ctrl+Shift+S` saves under a new name                        |
| `Space`                     | Play or pause                                                                 |
| `Esc`                       | Clears the selection                                                          |

On macOS, `Cmd` replaces `Ctrl`. The same actions are in the toolbars.

## Rules

A measure is always exactly full. Changing the length of a beat keeps it that way:

- **Shorter**: the freed time becomes rests right after the beat.
- **Longer**: the beats that follow are overwritten, notes included. If the measure has no room left
  after the beat, nothing changes and the app says so.
- A fret is 0 to 24. A dotted thirty-second note is not available.
- Inserting or deleting a measure applies to every track, so tracks always have the same length. The
  last measure cannot be deleted.

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

Ties and techniques (hammer-on, slide, bends) can be read and played but not yet entered. There is no
way to add a track, change the tuning or the time signature, select several notes, copy and paste, or
click on the score to place the playback cursor.
