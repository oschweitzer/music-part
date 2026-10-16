# MusicXML import and export

`io-musicxml` reads `score-partwise` files, plain (`.musicxml`, `.xml`) or compressed (`.mxl`, found
by its first bytes). `score-timewise` is refused. Reading is done with `roxmltree`, which bounds
the number of nodes and the expansion of entities; compressed files are read up to 128 MiB.

## What is read

| In the file                         | In the score                                                        |
| ----------------------------------- | ------------------------------------------------------------------- |
| `work-title`, `movement-title`      | Title (the file name when there is none)                            |
| Each `part` with notes              | A track                                                             |
| `unpitched` notes, `percussion`     | A drum track: see [`drums.md`](drums.md). The piece comes from the  |
| clef, or MIDI channel 10            | `midi-unpitched` of the note's `instrument`, else from its place    |
| `part-name`, `midi-program`         | Track name; the closest sound to the program, else from the name    |
| Part of no string instrument        | A piano track (pitched): see below                                  |
| `staff-tuning`                      | Tuning, highest string first. Else the standard tuning              |
| `divisions`, `time`                 | Positions; time signature of each measure ("3+2" beats are summed)  |
| `sound tempo`, quarter `metronome`  | Tempo of the score (the first one found), else 120                  |
| `transpose`                         | Notes are read at sounding pitch                                    |
| `string` and `fret` (technical)     | Kept as given. Other notes are placed on strings by pitch           |
| `chord`                             | Notes on distinct strings, low frets first                          |
| `tie type="stop"`                   | The note is tied to the one before                                  |
| `voice`, `backup`, `forward`        | One voice per (staff, voice), up to 16 per part; one for a piano    |
| `hammer-on`, `pull-off` (stop)      | The technique on the second note                                    |
| `slide` (start), `bend`, `harmonic` | Slide on the first note, bend, harmonic                             |
| `implicit="yes"` measure            | A pickup keeps its own, shorter, time signature                     |

## Decisions

- **Everything lands on a 32nd-note grid.** Starts and ends are rounded, so a triplet is
  approximated and every measure still adds up exactly. A beat whose length is not one note value
  becomes several tied beats.
- **Staff and tablature.** Guitar exports often hold the same notes twice, once in notation and
  once in tablature. When a part has a `TAB` clef only that staff is read.
- **String instrument or not.** A part is a string instrument when its `midi-program` is one
  (25 to 41 or 106, counted from 1 as the file does), else when its name says so ("guitar", "bass", "violin", "banjo", "steel"), else
  when it has a `TAB` staff. Any other part becomes a pitched track, the piano for now (see
  [`notation.md`](notation.md)): every pitch is kept, none is left out for want of a string.
- **The staves and voices of a pitched part are merged into one voice.** The editor and the staff
  show the first voice of a track, so a piano part read as two voices would lose its left hand on
  screen. Every note of the measure is cut where another starts or ends and the pieces are tied
  (`pitched_segments` in `score-model`), so the music sounds the same and nothing is shortened.
- **Repeats are not unrolled**, nor are endings, codas or jumps: each measure is read once.
- **Left out and counted** (the app tells the user): grace notes, notes no string can play,
  percussion notes in a pitched part, notes beyond 16 voices, notes squeezed out by overlapping
  voices or by the end of the measure.
- Parts of different lengths are padded with rests so that every track has the same number of
  measures. At most 64 parts and 5000 measures are read.

Techniques the model has but MusicXML has no standard way to say (palm mute) are not imported.

## Export

`export_musicxml` writes one part per track with a single `TAB` staff (`staff-details` with the
tuning, `string` and `fret` on every note). A pitched track is one standard staff, in the treble
clef, or in the bass clef when most of its notes are below middle C, with no `technical` element.
Durations use 32 divisions per quarter note, the unit of the score, so nothing is rounded and
importing the file gives the same music back. A test checks
this on the demo score and on a score with ties, techniques, a second voice and a changing time
signature. Hammer-ons and pull-offs are written as a start on the note before and a stop on the
note, slides as a start and a stop; palm mute is written as `other-technical` text. The file is
not compressed (no `.mxl`). A drum track is a part on a `percussion` clef and MIDI channel 10, with
`unpitched` notes at the place and with the head of their piece (see [`drums.md`](drums.md)).
