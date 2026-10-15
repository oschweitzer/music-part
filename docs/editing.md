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
|                             | With a selection, empties its notes. On a rest, takes the rest out (below)    |
| `Shift+Left` / `Shift+Right`| Selects several beats                                                         |
| `Ctrl+C` `Ctrl+X` `Ctrl+V`  | Copy, cut and paste the selected beats                                        |
| `W` `H` `Q` `E` `S` `T`     | Whole, half, quarter, eighth, sixteenth and thirty-second note                |
| `.`                         | Adds or removes a dot                                                         |
| `L`                         | Ties the note to the one before it on the same string (again: unties)         |
| `B` `M` `/`                 | Bend, palm mute, slide on the note (again: clears). Others are in the toolbar |
| `Ctrl+Enter`                | Inserts a measure after the selected one                                      |
| `Ctrl+Delete`               | Deletes the selected measure, or all the measures a range of beats touches    |
| `Alt+Up` / `Alt+Down`       | Moves the selection to the track above or below, at the same moment           |
| `Ctrl+Z`                    | Undo                                                                          |
| `Ctrl+Shift+Z` / `Ctrl+Y`   | Redo                                                                          |
| `Ctrl+N` `Ctrl+O` `Ctrl+S`  | New, open, save. `Ctrl+Shift+S` saves under a new name                        |
| `Space`                     | Play or pause                                                                 |
| `Home`                      | Moves playback back to the beginning                                          |
| `Esc`                       | Clears the selection                                                          |

On macOS, `Cmd` replaces `Ctrl`. The same actions are in the toolbars.

## Selecting with the mouse

Drag over the tablature to select, like text: a rectangle from where you pressed to where you
release, covering the beats between and the strings between. Drag across a whole chord to take all
its notes, or across part of it to take only those strings. `Delete`, cut and copy act on the
rectangle only (copy keeps just its strings). Shift+arrows and Shift+click select whole beats.

## Deleting rests

`Delete` first empties notes, so a beat left without notes is a rest. `Delete` on a rest (or on a
selection of rests only) takes it out of the timeline, the way deleting a character closes the gap
in text: what follows moves earlier and the measure gets shorter, and its time signature follows
(a 4/4 measure that loses a quarter becomes 3/4, one that loses an eighth becomes 7/8). A measure
with no time left disappears, so deleting its last rest deletes the measure.

The measures are the same length in every track, so the time is taken out of every track at once,
and only when all of them have rests there. If another track has a note in that time, `Delete`
does nothing and says so: no note is ever lost. Ctrl+Z undoes it.

## Copy and paste

`Shift+Left` and `Shift+Right` (or Shift+click, or dragging the mouse over the beats) select the beats from where you started to the beat
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

Each track tab, and each row header in the rows view, has `M` (mute) and `S` (solo) buttons. A track plays unless it is muted, but as soon
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
- Right-click the tablature for a menu: cut, copy, paste, insert a measure before or after, and
  delete the measure. When a range of beats is selected (Shift+arrows, or dragging the mouse) and
  the click is inside it, the menu acts on the whole range: "Delete 3 measures" removes every
  measure the range touches, in one undo step. A right-click outside the range selects that spot first.

## How techniques sound

The player turns techniques into MIDI (`audio-engine/src/sequencer.rs`):

| Technique           | Sound                                                                            |
| ------------------- | -------------------------------------------------------------------------------- |
| Hammer-on, pull-off | The note is played softly, and swells in without the attack of a pick            |
| Slide               | Glides, in the second half of the note, to the note on the same string that      |
|                     | follows (up to an octave away), which is then played softly. Nothing to reach:   |
|                     | a plain note                                                                     |
| Bend                | Rises a whole tone over the start of the note and is held until it ends          |
| Palm mute           | Quieter, dull, and cut to half its length; the string stops ringing at once      |
| Harmonic            | Natural harmonic: frets 12 and 24 an octave above the open string, 7 and 19 an   |
|                     | octave and a fifth, 5 two octaves, 4 two octaves and a third, 9 an octave and a  |
|                     | major third; any other fret an octave above the fretted note. Quieter and purer  |

### Timbres

MIDI cannot say "muted" or "no pick", so the sequencer gives each note a timbre (`Timbre`: open,
muted, harmonic, legato) and the player plays each timbre on its own MIDI channels. A channel is
shaped with SoundFont NRPN messages, which offset a generator of every note it plays:

| Timbre   | Used for                                   | Filter cutoff    | Other                          |
| -------- | ------------------------------------------ | ---------------- | ------------------------------ |
| Open     | everything else                            | unchanged        | unchanged                      |
| Muted    | palm mute                                  | 4 octaves down   | release about 20 times shorter |
| Harmonic | harmonic                                   | 2.5 octaves down | unchanged                      |
| Legato   | hammer-on, pull-off, note a slide lands on | 1 octave down    | attack of about 10 ms          |

The values are in `timbre_offsets` (`audio-engine/src/player.rs`) and were set by measuring the
rendered sound, not by ear: tests check that each timbre is duller than the open note, and that a
muted note dies when cut. They are a starting point, tune them by listening with `render_wav`.

Channels are handed out when a score is loaded: only a track that uses a timbre gets its
channels. A MIDI channel number is a byte, which leaves room for 53 such pairs; a score that needs
more plays the rest with the open timbre. The pitch wheel moves every channel of a track.

Bends and slides move the pitch wheel of the track's MIDI channel, so they bend every note sounding
on that track at the same time. A bend always rises a whole tone: the model has no bend amount yet.

## Track views

The `Tabs` / `Rows` switch in the toolbar changes how the tracks are shown. The choice is kept
between runs, like the theme.

- **Tabs**: one track at a time, picked from its tab. With many tracks the tabs scroll sideways, and
  the track menu next to the track fields lists them all.
- **Rows**: every track in a row of its own, one under the other, like the tracks of a DAW. Each
  row has a header on the left (name, instrument, number of strings, mute, solo, remove, and an
  arrow that folds the row down to its header) and its tablature on the right.

Selecting is the same in both: a click or a drag in a row selects in that row's track, and the
keys, paste and the track fields act on the track of the selection. A drag stays in the row it
started in, even when the mouse moves over another row. `Alt+Up` and `Alt+Down` move the selection
to the previous or next track, onto the beat that sounds at the same moment.

### Design

The view is described by *which tracks are shown and how* (`TrackView` and `shownTracks` in
`ui/src/lib/track-view.ts`), not by "the current track": tabs mode shows the track of the selection,
rows mode shows them all, folded or not. A later view (a few chosen instruments on screen at once)
is another way to fill that list; the layout and the rows already take any list of tracks. The
track being edited is where the selection is (`Editor.track`), separate from what is shown.

Rows use **one wide line per track** (DAW-like), not systems that wrap together as on a printed full
score. The measures are the same length in every track, so the rows share their barlines: measure n
is at the same x in every row, as wide as the track that needs most room, and notes that start at
the same moment are at the same x (`layoutRows` in `ui/src/lib/tab-layout.ts`). Reasons for one wide
line:

- With many tracks (up to 64), wrapped systems would each be taller than the screen, so reading one
  track across a line break would mean scrolling past all the others. One line per track keeps every
  track at the same height on screen, and folding rows works as expected.
- One playback cursor crosses every row; the view scrolls sideways to follow it.
- Only the rows and the measures on screen are drawn (`visibleRows`, `clipLayout`), so a long piece
  with many tracks stays light. A piece shorter than the window is stretched to its width.

The price is horizontal scrolling, and a printout of the rows view would not look like a score; the
tabs view, which wraps lines to the page, stays for that.

Folding a row and the view mode are not part of the score file. Folds are forgotten when the app
closes; the mode is remembered.

## Tracks and tuning

`+ Track` adds a nylon guitar, electric guitar, electric bass or violin in standard tuning with empty measures matching the first
track. The cross on a track's tab deletes it (the last track cannot be removed; Ctrl+Z brings a track back). The tuning field
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

Rows cannot be reordered by dragging, coloured, or given their own height yet, and there is no
mixer strip. There is no system clipboard, no select-all, and no copy between tracks of different voices (only the
first voice of a measure is edited).
