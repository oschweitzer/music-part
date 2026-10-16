# Notes as pitches, and the standard staff

Until [issue #8](https://github.com/oschweitzer/music-part/issues/8) every note was a string and a
fret, and every track a tablature. This note records how a track now says how its notes are
stored, how a pitched note is written in files and edited, how the staff is engraved, and how
the instruments still to come (keyboards, drums, voice) fit in.

## Model

### A note has a place

```rust
pub struct Note {
    #[serde(flatten)]
    pub place: Place,
    pub technique: Option<Technique>,
    #[serde(default)]
    pub tied: bool,
}

#[serde(untagged)]
pub enum Place {
    Fretted { string: u8, fret: u8 },
    Pitched { pitch: u8 },
}
```

- The **instrument decides the kind** (`Instrument::note_kind()`: `NoteKind::Fretted` or
  `NoteKind::Pitched`), and `Score::validate` checks that every note of a track is of that kind.
  A track cannot mix them. The piano is the only pitched instrument so far.
- `Note::key()` is what tells the notes of a beat apart: the string of a fretted note, the pitch of
  a pitched one. Beats keep their notes sorted by key, one note per key, and the UI selects a note
  by it (the selection *row*).
- `Track::pitch(note)` gives the sounding pitch of either kind, so the sequencer, the MIDI export
  and anything that only needs pitches does not care about the kind.
- A pitched track has no strings: its `tuning` is empty (checked by `validate`).

Two other designs were weighed. A note with both a string and fret and an optional pitch would
have changed less code, but allows notes that mean nothing (a piano note on a string) and would
need checks everywhere; the enum makes the compiler find every place that has to choose. A
separate kind of track for each family would duplicate measures, voices, beats and every edit
that works on rhythm (durations, paste, measures, rests), which are the same for every
instrument.

### File format

`flatten` and `untagged` keep the JSON flat, so a fretted note is written exactly as before and a
pitched note simply has `pitch` instead of `string` and `fret`:

```json
{ "string": 1, "fret": 3, "technique": null, "tied": false }
{ "pitch": 60, "technique": null, "tied": true }
```

Every file written before loads unchanged, so the format version stays 1 and this is not a
breaking change. A file with a piano track does not open in an older version of the app, as a
file with a banjo did not before the banjo existed. A note with neither a string nor a pitch is
refused when the file is read.

### Edits

The edits that name a string (`SetFret`, `RemoveNote`, `SetTied`, `SetTechnique`) stay for
fretted notes; `SetFret` is refused on a pitched track (`EditError::NoStrings`). Notes are also
addressed by the pitch they sound, on any track:

| Edit                    | On a pitched track                   | On a fretted track                        |
| ----------------------- | ------------------------------------ | ----------------------------------------- |
| `AddPitch`              | Adds the pitch to the beat (a chord) | Puts it on a free string, lowest fret     |
| `RemovePitch`           | Removes the note                     | Removes the note sounding at that pitch   |
| `MovePitch`             | Changes its pitch, unties it and the | Same string when it reaches the pitch,    |
|                         | note that continued it               | else another free string                  |
| `SetPitchTied`          | Ties to the same pitch on the beat   | Ties the note's string (`SetTied`)        |
|                         | before                               |                                           |
| `ClearNotes`, `pitches` | Clears the notes within a pitch span | Clears the notes sounding within the span |

- `Paste` keeps only the notes of the kind of the target track (a guitar chord pasted on a piano
  is left out, like notes on strings a track does not have).
- `SetInstrument` converts between kinds: fretted to pitched keeps every pitch and tie and drops
  techniques; pitched to fretted takes the standard tuning and places each chord on the strings
  (`place_chord`), refused with `NoFreeString` when a note cannot be placed.
- `SetTuning` is refused on a pitched track.

### Import and export

- `Sound::from_midi_program` gives the piano to every program that is not a string instrument here,
  so MIDI and MusicXML parts of keyboards, winds or voices keep all their notes instead of losing
  those no guitar string can play. A channel with no program at all stays a nylon guitar, since
  files written from a tablature often name none.
- A pitched track keeps notes whole: notes that overlap (a held bass note under a melody, the two
  hands of a piano part) are cut where another note starts or ends and tied on
  (`pitched_segments` in `score-model`), all in the one voice the editor shows. String tracks
  still cut a chord where the next one starts.
- Exports write pitched notes back: MIDI by pitch, MusicXML on a standard staff (treble, or bass
  for a mostly low part) without string or fret. See [`musicxml.md`](musicxml.md).

## The staff

`ui/src/lib/staff-layout.ts` engraves one measure of one staff as plain geometry, and
`tab-layout.ts` stacks the staves above the tablature in each system, on the same x for each
beat. `TabView.svelte` only draws; the glyphs (clefs, accidentals, rests, flags) are SVG paths in
`staff-glyphs.ts`, so no music font is needed.

| Track                          | Staves                         | Tablature           |
| ------------------------------ | ------------------------------ | ------------------- |
| Piano                          | Grand staff, split at middle C | None                |
| Guitar, electric guitar, banjo | Treble clef, an octave down    | `Tab` / `Both` view |
| Bass                           | Bass clef, an octave down      | `Tab` / `Both` view |
| Violin                         | Treble clef                    | `Tab` / `Both` view |
| Drums                          | Percussion clef (`drums.md`)   | None                |

- **Fretted tracks** show their tablature, their staff, or both (staff above), as the
  `Tab` / `Staff` / `Both` switch says (`FrettedView` in `track-view.ts`, kept like the view mode).
  Pitched tracks always show their staff.
- **Spelling** follows the key: notes of the key take its sharps or flats; in C major the black
  keys are C#, Eb, F#, G# and Bb. An accidental holds until the barline; a later natural note on
  the same line or space gets a natural sign. A note tied over the barline repeats no accidental.
- **Key signature** is data (`KeySignature`, number of sharps or flats) that the layout and the
  drawing support, drawn after the clef. The model has no key yet, so every score is in C major.
- **Ledger lines**, **stems** (up below the middle line, down from it; at least to the middle line),
  **seconds** in a chord on either side of the stem, **accidentals** in columns so close ones do not
  collide, **dots** in the space above a note on a line, **ties** between notes of the same pitch
  (from the start of the measure when the note before is in the previous one).
- **Beams** join eighths and shorter within a quarter note, flat, all stems of the group the same
  way. A lone short note has flags.
- **Rests** are drawn per staff: on a grand staff a beat with notes only in the right hand shows a
  rest on the bass staff.
- **Clicks**: `hitTest` gives the line or space under the mouse as a pitch of the key, and the note
  written there if there is one. On a pitched track the selection row is that pitch; on the staff
  of a fretted track it is the string of the note clicked (an empty spot keeps the string).
- **Rows view**: rows have different heights (a grand staff, staff and tablature) but share their
  measure columns, so barlines, notes and the cursor still line up across rows.

## Instruments to come

### Keyboards: organ, synthesizers, more pianos (issue #9)

Pitched instruments, nothing new in the model: add `Instrument::Organ`, `Synth`... with
`note_kind()` returning `Pitched`, an empty `standard_tuning`, their sounds in `sound.rs` (ids such
as `"organ/drawbar"`, `"synth/lead"`, `"piano/electric"`), the same rows in `ui/src/lib/instruments.ts`
(the Rust test `every_sound_is_listed_in_the_ui` checks them) and the instrument in `PITCHED` there,
and their clefs in `clefsFor` (`staff-layout.ts`). Move the General MIDI ranges they cover out of the
catch-all piano branch of `Sound::from_midi_program` (16 to 23 organs, 80 to 103 synthesizers,
1 to 7 other pianos as sounds of the piano), and the matching part names in `sound_for` of
`io-musicxml`.

### Drums (issue #10): done

Done, see [`drums.md`](drums.md). A drum note is a third kind of place,
`Place::Percussion { piece }` (the General MIDI key, 35 to 81), of `Instrument::Drums`
(`NoteKind::Percussion`). The pitched edits take the key of the piece, so `Note::key()`,
selection rows, `ClearNotes` spans and paste work unchanged; the track plays a kit of bank 128 on
MIDI channel 10, and is written on a percussion staff with a head shape per piece, stems up for
the hands and down for the feet alone, in one voice.

### Voice (issue #11)

A voice is pitched: `Instrument::Voice` with `NoteKind::Pitched`, the treble clef (or treble an
octave down for tenor and bass), and sounds from General MIDI 52 to 54 (choir aahs, voice oohs,
synth voice). Lyrics are the new part: an optional syllable per note, as a field with
`#[serde(default)]` on `Note` (older files stay valid), an edit to set it, and a line of text under
the staff drawn from the layout. Breath marks and slurs can follow the same way.

## Not done yet

- Only the first voice of a measure is shown and edited, so a pitched part is merged into one voice
  on import. Two voices on a staff (stems up and down) are not drawn.
- A note of the grand staff goes to the treble staff from middle C up: there is no way to put a
  note on the other staff by hand, and no beam crosses staves.
- Beams are flat; no slant, no beaming over rests, no tuplets.
- No key signature in the model, no clef change within a track, no octave lines (8va).
- Letter entry needs Shift (`Shift+A` to `Shift+G`): the plain letters are durations and
  techniques.
