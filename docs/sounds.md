# Instruments and sounds

A track has an **instrument** and a **sound**. The instrument is a family: it decides the strings,
how frets are counted, the standard tuning and how the track is written. The sound is how that
instrument sounds: a clean or a distorted electric guitar, a bass played with the fingers or
slapped. Changing the sound never touches the notes; changing the instrument can (see the banjo
below). This note covers Phase A of [issue #1](https://github.com/oschweitzer/music-part/issues/1):
string instruments, played with General MIDI programs.

## Model

- `Instrument` (`score-model`) stays the family: `Guitar`, `ElectricGuitar`, `Bass`, `Violin`,
  and now `Banjo`. `Guitar` is the acoustic guitar; its stored name predates the steel sound and
  is kept so that old files load.
- `Sound` is a field of `Track`, always one of the sounds of the track's instrument
  (`Score::validate` refuses anything else).
- **Steel-string guitar is a sound, not an instrument.** It has the strings, the tuning and the
  notation of the nylon guitar; only the timbre differs, which is exactly what a sound is. The
  banjo, on the other hand, has other strings, a re-entrant tuning and a short string: it is an
  instrument.

### Sound ids

In files and over IPC a sound is a stable id, `"<instrument>/<sound>"` in kebab-case, for
instance `"electric-guitar/distortion"`. The prefix names the family, so an id read alone says
which instrument it belongs to. In Rust the sounds are an enum serialized to these ids, which
keeps matches exhaustive; when sounds can come from packs (below) it can become a string newtype
without changing the file format.

- A track saved before sounds existed has no `sound`: it gets the default sound of its instrument,
  which is the sound it always had (`TrackFile` in `score-model`). The format version stays 1.
- An unknown id makes the file invalid, as an unknown instrument already does. Ids are therefore
  never renamed once released.

### The sounds

| Id                           | Name          | GM program | Instrument      | Sound pack file       |
| ---------------------------- | ------------- | ---------- | --------------- | --------------------- |
| `guitar/nylon`               | Nylon strings | 24         | Acoustic guitar | `nylon-guitar.sf2`    |
| `guitar/steel`               | Steel strings | 25         | Acoustic guitar |                       |
| `electric-guitar/clean`      | Clean         | 27         | Electric guitar | `electric-guitar.sf2` |
| `electric-guitar/muted`      | Muted         | 28         | Electric guitar |                       |
| `electric-guitar/overdrive`  | Overdrive     | 29         | Electric guitar |                       |
| `electric-guitar/distortion` | Distortion    | 30         | Electric guitar |                       |
| `bass/finger`                | Finger        | 33         | Electric bass   | `electric-bass.sf2`   |
| `bass/pick`                  | Pick          | 34         | Electric bass   |                       |
| `bass/slap`                  | Slap          | 36         | Electric bass   |                       |
| `violin/standard`            | Standard      | 40         | Violin          |                       |
| `banjo/standard`             | Standard      | 105        | Banjo           |                       |
| `piano/grand`                | Grand piano   | 0          | Piano           |                       |

Programs are General MIDI, counted from 0. The first sound of each instrument is its default.
`ui/src/lib/instruments.ts` mirrors this table by hand, and a Rust test
(`every_sound_is_listed_in_the_ui`) fails when a sound is missing there.

### Editing

- `Edit::SetSound { track, sound }` changes the sound, refused for a sound of another instrument.
- `Edit::SetInstrument` gives the track the default sound of its new instrument (setting the same
  instrument keeps the sound).
- `Edit::AddTrack` takes an optional `sound`; without it the track gets the default sound.

In the toolbar, the Sound menu next to the Instrument menu lists the sounds of the current
instrument. The "+ Track" menu lists every sound under its instrument.

## The banjo and its short fifth string

The five-string banjo is tuned G4 D3 G3 B3 D4 from the fifth string to the first, stored string 1
first: `[62, 59, 55, 50, 67]` (D4 B3 G3 D3 G4). The fifth string is short: its peg is at the
fifth fret of the neck. Banjo tablature numbers its frets like those of the other strings, so the
open fifth string is `0`, it has no frets 1 to 5, and its fret 6 is one semitone above the open
string.

The model follows that convention (`Instrument::first_fret`, used by `Track::pitch` and its
inverse `pitch_fret`): a note on frets 1 to 5 of that string has no pitch, so it is refused by
`Score::validate` and by the edits (`EditError::NoSuchFret`), the importers never place a note
there (`place_chord` knows the instrument), and changing a track to the banjo is refused if a note
would land there. The rule applies to a banjo with five strings; retuned to four (a tenor banjo)
every string runs the whole neck.

## Import and export

Both importers map a General MIDI program to the closest sound (`Sound::from_midi_program`):

| Program               | Sound                                                     |
| --------------------- | --------------------------------------------------------- |
| 24, and none          | `guitar/nylon` (a file for a tablature often names none)  |
| 25                    | `guitar/steel`                                            |
| 26, 27, 31            | `electric-guitar/clean` (jazz and harmonics: the closest) |
| 28, 29, 30            | `electric-guitar/muted`, `overdrive`, `distortion`        |
| 32, 33, 35, 38, 39    | `bass/finger` (acoustic, fretless and synth bass too)     |
| 34                    | `bass/pick`                                               |
| 36, 37                | `bass/slap`                                               |
| 40                    | `violin/standard`                                         |
| 105                   | `banjo/standard`                                          |
| any other             | `piano/grand`: keyboards, winds, voice, other strings...  |

- MIDI writes the program of the sound; reading it back gives the same sound.
- MusicXML writes it in `midi-program` (counted from 1). On import, a program of a string
  instrument wins; otherwise the part name decides ("electric guitar", "bass", "violin", "banjo",
  "steel", "guitar"), then a tablature staff (the nylon guitar), else the piano.
- A program that is not a string instrument gives a pitched track, the piano, which keeps every
  note (see [`notation.md`](notation.md)). Organ, synthesizers, drums and voice get their own
  instruments and sounds later; the mapping then sends their programs there.
- Tests export a track of every sound to MIDI, MusicXML and `.mpart` and check that the sound comes
  back.

## Playback

The sound's General MIDI program plays the track. A sound pack (see
[`sound-packs.md`](sound-packs.md)) replaces the program of the sounds it lists, not of a whole
instrument: the clean electric guitar plays the recorded Fender, the distorted one the General MIDI
program.

## Phase B: instruments without strings

Notes stored as pitches and the standard staff are in place, with one pitched instrument, the
piano (`piano/grand`, General MIDI program 0): see [`notation.md`](notation.md), which also says
how organ, synthesizers, drums and voice fit in.

## Follow-ups: sound packs and realistic samples

Not done in Phase A, recorded here so they are picked up later:

- **Sound packs: done** for the built-in sounds, see [`sound-packs.md`](sound-packs.md). Still to
  do: packs that declare new sounds (`Sound` then becomes a string id checked against the loaded
  table, and a track whose pack is missing falls back to the General MIDI program of the closest
  built-in sound), listing the packs and their licences in the UI, and loading only the packs a
  score uses.
- **SFZ.** Many free realistic libraries are SFZ, not SoundFont. Evaluate the sfizz library
  (C++, reported BSD-2-Clause; to be confirmed) behind the same `Player` interface, against the
  rules of `CLAUDE.md`: no allocation in the audio callback, replaceable library, licence
  compatible with PolyForm Noncommercial.
- **Realistic sample downloads.** More CC0 or permissive banks (steel guitar, distorted guitar,
  slap bass, violin, banjo) fetched by `pnpm fetch:soundfont` with pinned checksums. Hosts such as
  freepats could not be reached from the environment this was written in, so no new bank was
  added.
