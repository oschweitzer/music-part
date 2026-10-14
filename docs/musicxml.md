# MusicXML import

`io-musicxml` reads `score-partwise` files, plain (`.musicxml`, `.xml`) or compressed (`.mxl`, found
by its first bytes). `score-timewise` is refused. Reading is done with `roxmltree`, which bounds
the number of nodes and the expansion of entities; compressed files are read up to 128 MiB.

## What is read

| In the file                         | In the score                                                        |
| ----------------------------------- | ------------------------------------------------------------------- |
| `work-title`, `movement-title`      | Title (the file name when there is none)                            |
| Each `part` with pitched notes      | A track. Parts with only unpitched notes (drums) are left out       |
| `part-name`, `midi-program`         | Track name; guitar, bass or violin from the program or the name     |
| `staff-tuning`                      | Tuning, highest string first. Else the standard tuning              |
| `divisions`, `time`                 | Positions; time signature of each measure ("3+2" beats are summed)  |
| `sound tempo`, quarter `metronome`  | Tempo of the score (the first one found), else 120                  |
| `transpose`                         | Notes are read at sounding pitch                                    |
| `string` and `fret` (technical)     | Kept as given. Other notes are placed on strings by pitch           |
| `chord`                             | Notes on distinct strings, low frets first                          |
| `tie type="stop"`                   | The note is tied to the one before                                  |
| `voice`, `backup`, `forward`        | One voice per (staff, voice), up to 16 per part                     |
| `hammer-on`, `pull-off` (stop)      | The technique on the second note                                    |
| `slide` (start), `bend`, `harmonic` | Slide on the first note, bend, harmonic                             |
| `implicit="yes"` measure            | A pickup keeps its own, shorter, time signature                     |

## Decisions

- **Everything lands on a 32nd-note grid.** Starts and ends are rounded, so a triplet is
  approximated and every measure still adds up exactly. A beat whose length is not one note value
  becomes several tied beats.
- **Staff and tablature.** Guitar exports often hold the same notes twice, once in notation and
  once in tablature. When a part has a `TAB` clef only that staff is read.
- **Repeats are not unrolled**, nor are endings, codas or jumps: each measure is read once.
- **Left out and counted** (the app tells the user): grace notes, notes no string can play,
  percussion notes in a pitched part, notes beyond 16 voices, notes squeezed out by overlapping
  voices or by the end of the measure.
- Parts of different lengths are padded with rests so that every track has the same number of
  measures. At most 64 parts and 5000 measures are read.

Techniques the model has but MusicXML has no standard way to say (palm mute) are not imported.
