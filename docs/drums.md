# Drums

[Issue #10](https://github.com/oschweitzer/music-part/issues/10) adds drum tracks: a drum kit
whose notes are percussion pieces, written on a percussion staff and played on MIDI channel 10.
This note records the model, playback, import and export, and how drums are engraved and entered.

## Model

A drum note is a hit on a piece, not a pitch:

```rust
#[serde(untagged)]
pub enum Place {
    Fretted { string: u8, fret: u8 },
    Pitched { pitch: u8 },
    Percussion { piece: u8 },
}
```

- `Instrument::Drums` has `NoteKind::Percussion`, no strings and no tuning.
- `piece` is the General MIDI percussion key, 35 (acoustic kick) to 81 (open triangle)
  (`DRUM_PIECES`). The untagged JSON tells it apart by its field: `{"piece":38,...}`. Older files
  are untouched and the format version stays 1; a file with drums does not open in an older
  version of the app, as a file with a piano did not before the piano existed.
- `Note::key()` returns the piece, so beats keep their hits sorted and distinct, selection rows,
  `ClearNotes` spans and paste work unchanged. `Track::pitch` returns the key too, which is what
  the percussion channel plays, and `None` for a key that is no piece.
- `Score::validate`: pieces 35 to 81, each at most once per beat, no technique (techniques are
  for strings), no tuning.

### The piece table

`PIECES` in `score-model/src/percussion.rs` names every piece and says where it is written: the
line or space (as the note a treble staff has there, `display-step` and `display-octave` in
MusicXML) and the shape of its head. `ui/src/lib/drums.ts` mirrors it by hand; the Rust test
`every_piece_is_listed_in_the_ui` checks it. The kit follows the usual drum set notation:

| Pieces                                       | Line or space                 | Head               |
| -------------------------------------------- | ----------------------------- | ------------------ |
| Pedal hi-hat (44)                            | D4, under the staff           | x                  |
| Acoustic kick (35), kick (36)                | E4, F4 (bottom line, space)   | normal             |
| Floor toms (41, 43), low tom (45)            | G4, A4, B4                    | normal             |
| Snare (38), side stick (37), electric (40)   | C5 (third space)              | normal, x, diamond |
| Low-mid (47), high-mid (48), high tom (50)   | D5, E5, F5                    | normal             |
| Ride (51), ride bell (53), ride 2 (59)       | F5, F5, E5                    | x, diamond, x      |
| Closed (42), open hi-hat (46)                | G5, above the staff           | x, circled x       |
| Crash (49), splash (55), crash 2 (57), china | A5, A5, B5, C6 (ledger lines) | x, diamond, x, x   |

Hand percussion (39, 54, 56, 58, 60 to 81) has no common place on a drum staff: it takes the free
combinations of line and head (diamonds and triangles mostly), so that no two pieces share both.
A test checks that.

### Edits

The pitched edits take the key of the piece on a drum track, so no new edit was needed:

| Edit                      | On a drum track                                                         |
| ------------------------- | ----------------------------------------------------------------------- |
| `AddPitch`                | Strikes the piece on the beat; a key that is no piece is refused        |
| `RemovePitch`             | Takes the piece away                                                    |
| `MovePitch`               | Moves the hit to another piece (the arrows: the next line of the staff) |
| `SetPitchTied`            | Ties the hit to the same piece on the beat before (it is not struck)    |
| `ClearNotes`, `pitches`   | Clears the pieces whose keys are in the span                            |
| `Paste`                   | Keeps only drum notes, without technique                                |
| `SetFret`, `SetTuning`    | Refused: `NoStrings`                                                    |
| `SetInstrument` to drums  | Only for a track without notes (`NotesOfAnotherKind`): a piece is no    |
| or from drums             | pitch, so nothing converts between them                                 |

`EditError::InvalidPiece` names a key outside 35 to 81.

## Sounds and playback

Drums have six sounds, the General MIDI 2 / GS drum kits of the SoundFont's bank 128:

| Id                 | Name           | Kit |
| ------------------ | -------------- | --- |
| `drums/standard`   | Standard kit   | 0   |
| `drums/room`       | Room kit       | 8   |
| `drums/power`      | Power kit      | 16  |
| `drums/electronic` | Electronic kit | 24  |
| `drums/jazz`       | Jazz kit       | 32  |
| `drums/brush`      | Brush kit      | 40  |

`Sound::midi_program` gives the kit, which is the program of the percussion channel, not a
melodic program; `Sound::from_drum_kit` reads it back (the kits in between go to the group of
eight they belong to, the TR-808 to electronic, others to standard).

- **Sequencer.** A drum track is scheduled apart (`schedule_drums`): every hit at the default
  velocity and the open timbre, and a hit rings until the same piece is struck again or the
  track ends, instead of stopping at the end of its beat. A crash written as an eighth note is
  not choked; the kit's own release and exclusive classes (the closed hi-hat cutting the open
  one) shape the sound, as on a drum machine.
- **Player.** The first drum track plays on MIDI channel 10 (index 9), which melodic tracks
  skip; any other drum track plays on its own channel. Every drum channel selects bank 128 with
  `Synth::select_bank` (a bank select message only carries 0 to 127) and the kit as program.
  Mute, solo and seek work as for any track, since they act on the track's channels.
- **Sound packs and SFZ banks never play drums.** A pack that lists a drum kit is refused
  ("drum kits cannot come from a sound pack yet"), and the player ignores any bank for a drum
  track: a pack records one instrument across its keys, while a kit maps every key to a piece.

## Import and export

- **MIDI.** Channel 10 becomes a drum track (it was left out before), its program picking the
  kit; a program on that channel never picks a melodic instrument. Drum notes are often a few
  ticks long, so a hit lasts until the next hit, and at least a quarter note when nothing
  follows soon (`drum_segments` in `score-model`); keys that are no piece are left out and
  counted. Export writes every drum track on channel 10 with its kit as program.
- **MusicXML.** A part of `unpitched` notes, a part with a `percussion` clef, or one whose
  instrument plays on MIDI channel 10, becomes a drum track; its `midi-program` picks the kit.
  Each note's piece is the `midi-unpitched` key of its `instrument`, else the piece written at
  its `display-step` / `display-octave` with its `notehead`. All voices (hands and feet are often
  two) merge into one, each hit lasting until the next. Ties on drum notes are not read back:
  the hit is not struck again either way. Export writes a `percussion` clef, a `score-instrument`
  and `midi-instrument` (channel 10, kit, `midi-unpitched`) per piece used, and `unpitched` notes
  with their place, `notehead`, `instrument` and stem (down for the feet alone). Tests round-trip
  every piece through MIDI, MusicXML (with and without instruments) and `.mpart`.

## The staff

`clefsFor('Drums')` is the percussion clef, a five-line staff with the lines of a treble staff and
no key signature. `tab-layout.ts` gives each drum note its fixed position and head from
`drums.ts` (`StaffNoteInput.position`, `.head`), so `engraveMeasure` neither spells it nor draws
an accidental. `TabView.svelte` draws the clef as two bars and the heads as ovals, x (cymbals,
hi-hats), circled x (open hi-hat), diamonds or triangles.

- **Stems** go up for the hands and down for a beat of feet only (kicks, pedal hi-hat): the beat
  gives `stem` to `engraveMeasure`. A beam group with a hand goes up.
- **One voice.** Proper two-voice drum notation (hands up and feet down at once, with their own
  beams and rests) needs the second voice the editor does not show yet: a beat with both hands and
  feet has one stem, up. Pieces that share a line and are struck together (rare: a conga and a
  bongo) overlap.

## Entering drums

There are no frets or pitches to type, so drums have their own entry:

- **Digits** strike a piece of the palette on the selected beat, or take it away when the beat has
  it: `1` kick, `2` snare, `3` closed hi-hat, `4` open hi-hat, `5` pedal hi-hat, `6` crash,
  `7` ride, `8` high tom, `9` mid tom, `0` floor tom. On a drum track digits have nothing else to
  do, so no other key is taken: the plain letters are durations and techniques, and `Shift` with
  a letter names a note (on drums the app says how to write instead).
- The **palette** in the toolbar, shown for a drum track, has the same pieces (lit when the beat
  has them) and a **More…** list with every other piece.
- **Double-click** a line or space writes the drum of that line, else its cymbal (C5 the snare,
  G5 the closed hi-hat). **Enter** strikes the selected piece.
- **Up / Down** on a hit move it one line or space, to the drum of that line (else its cymbal,
  else another of its pieces), skipping the pieces the beat already has; on an empty spot they
  move the selection the same way. The other pieces of a line are a digit or a click away.
- Durations, dots, `L` (tie), `Delete`, copy, cut and paste work as on any track.
