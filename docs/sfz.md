# SFZ sound packs

Many free realistic sample libraries are published as [SFZ](https://sfzformat.com/): a text file
of regions (which sample plays for which key and velocity, how it loops, its envelope) next to a
folder of WAV files. Issue #6 lets a sound pack (see [`sound-packs.md`](sound-packs.md)) list an
`.sfz` file instead of an `.sf2`, played behind the same `Player` interface, with the notes outside
the recorded range still played by the General MIDI program.

## Options

### (a) sfizz, the reference SFZ player

[sfizz](https://github.com/sfztools/sfizz) is the most complete open SFZ engine (C++17).

- **Licence**: BSD-2-Clause, permissive: it can be combined with PolyForm Noncommercial.
- **Rust bindings**: none on crates.io (`sfizz` and `sfizz-sys` do not exist, checked in October
  2026). We would write and maintain a `-sys` crate: `bindgen` over its C API (`sfizz.h`) and a
  `build.rs` driving CMake.
- **Building on CI**: a CMake build of a C++17 library and its vendored dependencies on Linux,
  Windows (MSVC) and macOS. Our CI sets up no C++ build or CMake today, and every clean
  `cargo build` would rebuild it: minutes, not seconds.
- **Real-time safety**: designed for plug-ins, with preallocated voices and a render that does not
  allocate. It streams samples from disk with its own background threads, a second threading
  model next to ours.
- **Binary size**: several megabytes of native code, for an app that wants to stay light.
- **Coverage**: nearly all of SFZ v2 and the ARIA extensions, far more than a tablature player
  needs now.

### (b) A minimal SFZ player in Rust

A parser and a voice pool inside `audio-engine`, covering the opcodes that guitar and bass
libraries actually use: regions and their inheritance, key and velocity ranges, round-robin,
loops, the amplitude envelope, volume, pan and tuning, and WAV decoding.

- No new dependency: WAV (PCM and float) is decoded by a small reader of our own, which also
  checks every size before allocating. FLAC (`claxon`, Apache-2.0) or Ogg Vorbis (`lewton`,
  MIT/Apache-2.0) can be added later behind the same function.
- Pure Rust, so nothing changes in CI on the three systems.
- Samples are decoded once when the pack is loaded, away from the audio thread. The audio callback
  only reads them and updates a fixed pool of voices.
- Whatever we do not support is counted and reported, never a panic.

Existing Rust crates were looked at: `sofiza` (MIT/Apache-2.0) only parses, has not changed since
2022, and does not check paths or sizes, so it would not save much.

## Decision

**(b), a minimal SFZ player in Rust** (`crates/audio-engine/src/sfz/`). sfizz is not "clearly
easy": no bindings exist, and it would add a C++ and CMake build to all three CI systems and
several megabytes to the app, for an opcode set we mostly would not use. The subset below plays
the typical multi-sampled guitar or bass library; sfizz stays the option if packs ever need the
rest of the format.

## Manifest

A sound of a pack names its file by extension: `.sf2` or `.sfz`. The manifest `format` stays `1`:
no field was added, so a manifest written for this version is still read by this version's
parser, and an older app refuses an `.sfz` file with a clear message ("only SoundFont (.sf2)
files are supported") rather than misreading it.

```json
{ "sound": "guitar/nylon", "file": "nylon.sfz", "firstKey": 40, "lastKey": 88 }
```

`firstKey` and `lastKey` mean the same as for a SoundFont: a note outside is played by the
General MIDI program. A note inside the range that no region answers (a gap in the keys or
velocities) is also played by the General MIDI program rather than left silent.

## What is supported

Headers: `<control>`, `<global>`, `<master>`, `<group>`, `<region>`. An opcode set at a level
applies to every region under it, a region's own value winning.

| Opcode                                                | Meaning                                                  |
| ----------------------------------------------------- | -------------------------------------------------------- |
| `default_path` (in `<control>`)                       | Folder prefixed to every `sample`                        |
| `sample`                                              | The WAV file, relative to the `.sfz` file                |
| `lokey`, `hikey`, `key`                               | Key range (`key` also sets `pitch_keycenter`)            |
| `lovel`, `hivel`                                      | Velocity range                                           |
| `lorand`, `hirand`                                    | Random choice between layers (0 to 1)                    |
| `seq_length`, `seq_position`                          | Round-robin                                              |
| `pitch_keycenter`, `pitch_keytrack`                   | Key the sample was recorded at, cents per key            |
| `transpose`, `tune`                                   | Semitones, cents                                         |
| `volume`, `amplitude`, `pan`, `amp_veltrack`          | Gain in dB and percent, pan -100 to 100, velocity curve  |
| `ampeg_delay`, `ampeg_attack`, `ampeg_hold`           | Amplitude envelope, in seconds                           |
| `ampeg_decay`, `ampeg_sustain`, `ampeg_release`       | Decay and release in seconds, sustain in percent         |
| `offset`, `end`                                       | First and last sample frame played                       |
| `loop_mode`                                           | `no_loop`, `one_shot`, `loop_continuous`, `loop_sustain` |
| `loop_start`, `loop_end` (`loopstart`, `loopend`)     | Loop points in frames, else the WAV `smpl` loop          |
| `trigger=attack`                                      | The default; other triggers drop the region (see below)  |

Keys are numbers or note names (`c4` is 60, `c#4` and `db4` are 61). `#define $NAME value` is
expanded. Comments (`//` and `/* */`) are skipped.

Samples: WAV, 8, 16, 24 or 32-bit integer PCM or 32 or 64-bit float, mono or stereo, any sample
rate (resampled with linear interpolation at the output rate).

## What is ignored, and how

Everything else is **ignored and counted**, never a failure: the console prints each pack's SFZ
report, for instance `nylon.sfz: 48 regions, 48 samples, 21.3 MB; ignored: 12 × amp_velcurve_127,
4 × off_by`. In particular, not yet:

- key switches (`sw_*`) and other conditions on controllers (`locc`, `hicc`...): a later step for
  articulations;
- release triggers (`trigger=release`), choke groups (`group`, `off_by`), filters and LFOs
  (`fil_*`, `cutoff`, `lfo*`), other envelopes (`pitcheg_*`, `fileg_*`), modulation by velocity
  or controller (`*_veltrack` other than `amp_veltrack`, `*_oncc*`);
- `#include`, `<curve>`, `<effect>`, `<midi>` and `<sample>` headers (embedded samples);
- generated samples (`sample=*sine`...): the region is dropped;
- FLAC and Ogg files: the region is dropped, as for a missing WAV.

A region whose sample is missing, unreadable or in an unsupported format is dropped and counted;
an instrument left with no region is refused as a whole, and that sound plays its General MIDI
program.

## Safety

The `.sfz` file and its samples are user input:

- **Paths stay in the pack folder.** `default_path` and `sample` are joined, `\` read as `/`, and
  refused when absolute, with a drive letter, or with a `..` component; the resolved file must
  also be inside the pack folder once symbolic links are followed. A path that escapes refuses the
  whole `.sfz` file: such a pack is not a mistake to work around.
- **Memory is capped.** The `.sfz` text is read up to 1 MiB (4 MiB once `#define`s are expanded),
  at most 8192 regions and 2048 sample files. The decoded samples of one pack, all its `.sfz`
  files together, are capped at 256 MiB (about 25 minutes of mono 44.1 kHz audio, as `f32`): a
  sample that would go over is dropped and counted. Every size read from a WAV header is checked
  against the file length and the budget before anything is allocated.
- **No panic.** Bad numbers, unknown opcodes, broken WAV chunks are errors or counts.

## Playback

`Player::add_sfz` gives an `SfzInstrument` to a sound; a later SoundFont or SFZ for the same sound
replaces it, as with SoundFonts. When a note of that sound starts:

1. a key outside `firstKey` to `lastKey` goes to the General MIDI program;
2. otherwise every region whose key, velocity, random and round-robin conditions match starts a
   voice; if none matches, the note goes to the General MIDI program.

The voices are mixed with the `oxisynth` output in `Player::render`, frame by frame.

**Real time.** The voice pool (64 voices) is allocated when the player is created, and the round-
robin counters when the instrument is loaded. `render` and the event dispatch only read samples
and write into these fixed arrays: no allocation, no lock, no blocking call. When the pool is full
the oldest voice is taken over.

**Per-track features.** A voice remembers its track: a pitch bend moves the voices of its track;
muting a track (mute or solo) releases its voices; pause and stop release every voice, seeking and
loading a score cut them.

**Techniques.** The SoundFont path offsets generators of the synthesizer; the SFZ voices do the
same with their own envelope and a one-pole low-pass filter:

| Timbre   | SFZ voice                                                                   |
| -------- | --------------------------------------------------------------------------- |
| Open     | As the `.sfz` file says                                                     |
| Muted    | Low-pass at 1.2 kHz, release a twentieth as long: dull and short            |
| Harmonic | Low-pass at 3.5 kHz: few overtones                                          |
| Legato   | Attack of at least 10 ms and low-pass at 10 kHz: no pick attack             |

Technique-specific samples (a palm-muted recording) need key switches, a later step.

**Envelope.** Attack is linear; decay and release are exponential, reaching -60 dB in the given
time; a release is at least 5 ms to avoid clicks. `one_shot` regions ignore note-offs. A voice
whose sustain level is 0 is freed once its decay is over. The test
`playing_sfz_voices_does_not_allocate` (`audio-engine/tests/sfz_real_time.rs`) counts the allocations made while notes start, stop, bend
and render, and expects none.

**Loudness.** A region at `volume=0` and velocity 127 plays its sample at half its recorded
level (`SFZ_GAIN`), close to the level of the General MIDI programs; velocity follows the SFZ
default curve (gain `(velocity / 127)²` at `amp_veltrack=100`).

Code: `audio-engine/src/sfz/` (`parse.rs` the text, `wav.rs` the samples, `mod.rs` loading and
limits, `voice.rs` the voice pool), wired in `player.rs` and `pack.rs`.
