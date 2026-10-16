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
- `Sound` is a field of `Track`, always a sound of the track's instrument (`Score::validate`
  refuses anything else): one of the built-in table below, or one a sound pack declares (see
  [`sound-packs.md`](sound-packs.md#new-sounds)).
- **Steel-string guitar is a sound, not an instrument.** It has the strings, the tuning and the
  notation of the nylon guitar; only the timbre differs, which is exactly what a sound is. The
  banjo, on the other hand, has other strings, a re-entrant tuning and a short string: it is an
  instrument.

### Sound ids

In files and over IPC a sound is a stable id, `"<instrument>/<sound>"` in kebab-case, for
instance `"electric-guitar/distortion"`. The prefix names the family (`Instrument::sound_prefix`),
so an id read alone says which instrument it belongs to.

In Rust, `Sound` is a checked id rather than an enum, so that packs can add sounds without a
change of `score-model`. It keeps the id inline (at most 62 bytes), which makes it `Copy` and lets
the audio callback compare sounds without allocating. The built-in sounds are a static table,
`BUILT_IN_SOUNDS`, with a constant each (`Sound::GUITAR_NYLON`); their name and program come from
that table. The file format did not change: a built-in sound is written exactly as before.

- A track saved before sounds existed has no `sound`: it gets the default sound of its instrument,
  which is the sound it always had (`TrackFile` in `score-model`). The format version stays 1.
- `score-model` does not know the packs, so it cannot tell a sound of a pack that is not installed
  from a mistake. It accepts any well-formed id whose prefix is the track's instrument, and refuses
  an id that is malformed, names no instrument, or names another instrument than the track's. A
  score made with a pack therefore opens without it (see
  [`sound-packs.md`](sound-packs.md#a-score-whose-pack-is-missing)).
- Drum kits are the exception: an unknown `drums/...` id is refused, since packs never play kits
  (see [`drums.md`](drums.md)).
- Built-in ids are never renamed once released: files name them.

### The sounds

| Id                           | Name              | GM program | Instrument      | Sound pack file       |
| ---------------------------- | ----------------- | ---------- | --------------- | --------------------- |
| `guitar/nylon`               | Nylon strings     | 24         | Acoustic guitar | `nylon-guitar.sf2`    |
| `guitar/steel`               | Steel strings     | 25         | Acoustic guitar |                       |
| `electric-guitar/clean`      | Clean             | 27         | Electric guitar | `electric-guitar.sf2` |
| `electric-guitar/muted`      | Muted             | 28         | Electric guitar |                       |
| `electric-guitar/overdrive`  | Overdrive         | 29         | Electric guitar |                       |
| `electric-guitar/distortion` | Distortion        | 30         | Electric guitar |                       |
| `bass/finger`                | Finger            | 33         | Electric bass   | `electric-bass.sf2`   |
| `bass/pick`                  | Pick              | 34         | Electric bass   |                       |
| `bass/slap`                  | Slap              | 36         | Electric bass   |                       |
| `violin/standard`            | Standard          | 40         | Violin          |                       |
| `banjo/standard`             | Standard          | 105        | Banjo           |                       |
| `piano/grand`                | Grand piano       | 0          | Piano           |                       |
| `piano/bright`               | Bright piano      | 1          | Piano           |                       |
| `piano/electric`             | Electric piano    | 4          | Piano           |                       |
| `piano/electric-fm`          | FM electric piano | 5          | Piano           |                       |
| `organ/drawbar`              | Drawbar           | 16         | Organ           |                       |
| `organ/rock`                 | Rock              | 18         | Organ           |                       |
| `organ/church`               | Church            | 19         | Organ           |                       |
| `organ/reed`                 | Reed              | 20         | Organ           |                       |
| `synth/square-lead`          | Square lead       | 80         | Synthesizer     |                       |
| `synth/saw-lead`             | Saw lead          | 81         | Synthesizer     |                       |
| `synth/warm-pad`             | Warm pad          | 89         | Synthesizer     |                       |
| `synth/choir-pad`            | Choir pad         | 91         | Synthesizer     |                       |
| `voice/aahs`                 | Choir aahs        | 52         | Voice           |                       |
| `voice/oohs`                 | Voice oohs        | 53         | Voice           |                       |
| `voice/synth`                | Synth voice       | 54         | Voice           |                       |
| `drums/standard`             | Standard kit      | 0 (kit)    | Drums           |                       |
| `drums/room`                 | Room kit          | 8 (kit)    | Drums           |                       |
| `drums/power`                | Power kit         | 16 (kit)   | Drums           |                       |
| `drums/electronic`           | Electronic kit    | 24 (kit)   | Drums           |                       |
| `drums/jazz`                 | Jazz kit          | 32 (kit)   | Drums           |                       |
| `drums/brush`                | Brush kit         | 40 (kit)   | Drums           |                       |

Programs are General MIDI, counted from 0. The first sound of each instrument is its default. A
drum kit's program is the kit on the percussion channel (bank 128 of the SoundFont), not a melodic
program, and sound packs never play kits (see [`drums.md`](drums.md)).

The keyboards, the synthesizer and the voice ([issues #9](https://github.com/oschweitzer/music-part/issues/9)
and [#11](https://github.com/oschweitzer/music-part/issues/11)) keep short lists of sounds that are
clearly apart in a General MIDI bank: the percussive organ (17) is close to the drawbar one and the
other pads to the warm pad, so they are played by those. A test in `audio-engine` renders the same
chord with each of these sounds in the base SoundFont and checks that it is heard, that every sound
picks its own preset and that no two renderings are the same. A realistic piano pack (such as
Salamander) belongs to the sample banks of [issue #7](https://github.com/oschweitzer/music-part/issues/7).
`ui/src/lib/instruments.ts` mirrors this table by hand, and a Rust test
(`every_sound_is_listed_in_the_ui`) fails when a sound is missing there.

### Editing

- `Edit::SetSound { track, sound, fallbackProgram }` changes the sound, refused for a sound of
  another instrument. For a pack's sound the app fills `fallbackProgram` from the pack that
  declares it, and refuses a sound no pack declares (unless the track already has it).
- `Edit::SetInstrument` gives the track the default sound of its new instrument (setting the same
  instrument keeps the sound).
- `Edit::AddTrack` takes an optional `sound`; without it the track gets the default sound.

In the toolbar, the Sound menu next to the Instrument menu lists the sounds of the current
instrument, then those the packs declare for it, then the track's own sound when no pack has it
("twelve-string (pack missing), plays General MIDI program 25"). The "+ Track" menu lists every
sound under its instrument, pack sounds included.

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

| Program            | Sound                                                               |
| ------------------ | ------------------------------------------------------------------- |
| 24, and none       | `guitar/nylon` (a file for a tablature often names none)            |
| 25                 | `guitar/steel`                                                      |
| 26, 27, 31         | `electric-guitar/clean` (jazz and harmonics: the closest)           |
| 28, 29, 30         | `electric-guitar/muted`, `overdrive`, `distortion`                  |
| 32, 33, 35, 38, 39 | `bass/finger` (acoustic, fretless and synth bass too)               |
| 34                 | `bass/pick`                                                         |
| 36, 37             | `bass/slap`                                                         |
| 40                 | `violin/standard`                                                   |
| 105                | `banjo/standard`                                                    |
| 0                  | `piano/grand`                                                       |
| 1, 2, 3, 6         | `piano/bright` (electric grand, honky-tonk, harpsichord)            |
| 4, 7               | `piano/electric` (and the clavinet)                                 |
| 5                  | `piano/electric-fm`                                                 |
| 16, 17             | `organ/drawbar` (and the percussive organ)                          |
| 18, 19             | `organ/rock`, `organ/church`                                        |
| 20 to 23           | `organ/reed` (accordion, harmonica: free reeds too)                 |
| 52, 53, 54         | `voice/aahs`, `voice/oohs`, `voice/synth`                           |
| 80, 82, 83         | `synth/square-lead` (calliope and chiff leads too)                  |
| 81, 84 to 87       | `synth/saw-lead` (the other leads)                                  |
| 91                 | `synth/choir-pad`                                                   |
| 88 to 103 but 91   | `synth/warm-pad` (the other pads and the effects)                   |
| any other          | `piano/grand`: chromatic percussion, winds, brass, other strings... |

- MIDI writes the program of the sound; reading it back gives the same sound. A pack's sound is
  written with its fallback program, so it comes back as the built-in sound of that program.
- MusicXML writes it in `midi-program` (counted from 1). On import, a program that has a sound
  of its own family wins (a string instrument, a keyboard other than the grand piano, a
  synthesizer, a voice); the grand piano and the programs of no family come after the part name,
  since some programs write the piano for every part. The name then decides: a voice ("voice",
  "vocals", "soprano", "alto", "tenor", "baritone", "bass voice", "choir"..., but not "alto sax"
  or "tenor banjo"), an organ ("organ", "church organ", "accordion"), a synthesizer ("synth",
  "synth pad"), an electric piano ("electric piano", "Rhodes"), then "electric guitar", "bass",
  "violin", "banjo", "steel", "guitar". "Bass" alone stays the bass guitar. Then a tablature staff
  (the nylon guitar), else the grand piano.
- MIDI files are read by program only: a track name says too little to override it.
- A program of no family here gives a pitched track, the grand piano, which keeps every note
  (see [`notation.md`](notation.md)).
- Drums are not chosen by a program but by the percussion channel (MIDI channel 10); the program
  on that channel picks the kit (`Sound::from_drum_kit`). See [`drums.md`](drums.md).
- Tests export a track of every sound to MIDI, MusicXML and `.mpart` and check that the sound comes
  back.

## Playback

The sound's General MIDI program plays the track (`Track::midi_program`: for a pack's sound, the
fallback program the track recorded). A sound pack (see [`sound-packs.md`](sound-packs.md))
replaces the program of the sounds it lists, not of a whole instrument: the clean electric guitar
plays the recorded Fender, the distorted one the General MIDI program.

## Phase B: instruments without strings

Notes stored as pitches and the standard staff are in place, with one pitched instrument, the
piano (`piano/grand`, General MIDI program 0): see [`notation.md`](notation.md), which also says
how organ, synthesizers, drums and voice fit in.

## Follow-ups: sound packs and realistic samples

Not done in Phase A, recorded here so they are picked up later:

- **Sound packs: done**, see [`sound-packs.md`](sound-packs.md): packs give samples to the
  built-in sounds or declare new ones, the `Sounds` panel lists them with their licences, and only
  the packs a score uses are loaded.
- **SFZ: done** as a subset played by our own Rust code, after evaluating sfizz; see
  [`sfz.md`](sfz.md). Still to do: key switches for technique-specific samples, release
  samples, FLAC and Ogg samples.
- **Realistic sample downloads.** More CC0 or permissive banks (steel guitar, distorted guitar,
  slap bass, violin, banjo) fetched by `pnpm fetch:soundfont` with pinned checksums. Hosts such as
  freepats could not be reached from the environment this was written in, so no new bank was
  added.
