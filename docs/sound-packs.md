# Sound packs

A sound pack gives recordings to one or more sounds of the built-in table (see
[`sounds.md`](sounds.md)): a nylon guitar recorded from a real guitar instead of the General MIDI
program. A pack is a folder, so adding one needs no code: drop the folder in the packs directory
and restart the app.

## What a pack is

```
freepats-nylon-guitar/
├── pack.json       # the manifest, below
├── LICENSE.txt     # the licence of the samples, in full
└── nylon-guitar.sf2
```

`pack.json`:

```json
{
  "format": 1,
  "id": "freepats-nylon-guitar",
  "name": "Spanish classical guitar",
  "author": "roberto@zenvoid.org for FreePats",
  "licence": "CC0-1.0",
  "source": "https://freepats.zenvoid.org/...",
  "sounds": [
    { "sound": "guitar/nylon", "file": "nylon-guitar.sf2", "firstKey": 29, "lastKey": 88 }
  ]
}
```

| Field               | Meaning                                                                         |
| ------------------- | ------------------------------------------------------------------------------- |
| `format`            | Version of the manifest format, `1`. A newer one is refused with a message.     |
| `id`                | Stable id: lowercase letters, digits and dashes. The folder has the same name.  |
| `name`, `author`    | Shown to the user, not empty.                                                   |
| `licence`           | SPDX identifier of the samples' licence (`CC0-1.0`, `CC-BY-4.0`...).            |
| `source`            | Optional: where the samples come from.                                          |
| `sounds[].sound`    | A sound id of the built-in table, such as `"bass/slap"`. Each one at most once. |
|                     | Not a drum kit: a pack that lists one is refused (see [`drums.md`](drums.md)).  |
| `sounds[].file`     | A SoundFont (`.sf2`) or SFZ (`.sfz`) file in the pack folder, by its file name. |
| `sounds[].firstKey` | First MIDI key the recordings cover.                                            |
| `sounds[].lastKey`  | Last MIDI key the recordings cover.                                             |

The SoundFont's first preset is played. An `.sfz` file is played by the app's own SFZ player, which
reads a subset of the format and samples in WAV: see [`sfz.md`](sfz.md). Notes outside `firstKey`
to `lastKey` are played with the General MIDI program of the sound rather than left silent: the
bass recording stops at A2, the higher notes come from the base SoundFont. A manifest with an
unknown field, a file outside its folder or a key range that is not one is refused as a whole.

## Where the app looks

1. The **bundled packs**, in the app's resources (`soundfont/packs/`). In development, the
   `MUSIC_PART_PACKS` environment variable replaces this folder.
2. The **user's packs**, in `packs/` under the app data directory:
   - Linux: `~/.local/share/dev.musicpart.app/packs/`
   - macOS: `~/Library/Application Support/dev.musicpart.app/packs/`
   - Windows: `%APPDATA%\dev.musicpart.app\packs\`

   The folder name is the `identifier` of `src-tauri/tauri.conf.json`.

Every subfolder holding a `pack.json` is a pack; folders are read in name order. A user's pack
with the id of a bundled one replaces it, and when two packs play the same sound the one read last
is heard (user packs after bundled ones). A pack or a file that cannot be used is reported on the
console and skipped: its sounds keep their General MIDI program (or the previous pack with the
sound), and the app starts anyway.

The `Sounds` button of the toolbar shows the packs in use with their author and licence, the ones
refused and why, which pack or General MIDI plays the sound of the selected track, and the user's
packs folder. The console lists the same.

Code: `audio-engine/src/pack.rs` (manifest, discovery), `audio-engine/src/banks.rs` (loading on
demand), `src-tauri/src/lib.rs` (which folders, at start) and `src-tauri/src/audio.rs` (the audio
thread that reads the samples).

## When the samples are loaded

Only the manifests are read at start. A pack's sample file is read when a track of the score uses
one of its sounds, and its bank is removed from the synthesizer when no track uses it any more:

1. Each time the score changes, the audio thread (not the real-time callback) asks `PackBanks`
   which banks to add and remove. It reads the new files there, so neither the window nor the
   sound waits for the disk.
2. The changes go to the audio callback as messages, before the score that needs them, and only
   take effect when that score is loaded. The samples of a removed bank are sent back and freed on
   the audio thread, not in the callback.
3. A file that cannot be read is reported once and not read again; the sound then plays with the
   previous pack that has it, or with General MIDI.

An `.sfz` file goes the same way: it is parsed and its WAV samples decoded on the audio thread
when a track first uses its sound, and the instrument is freed there when none does. The decoded
samples of the SFZ files of one pack loaded at the same time share the pack's memory cap (see
[`sfz.md`](sfz.md)). A refused `.sfz` is listed with the refused files; what a loaded one left
out (opcodes, missing samples) is printed on the console. A SoundFont and an SFZ bank for the same
sound follow one rule: the one added last plays it, and removing it brings back the other.

`oxisynth` removes a SoundFont cleanly (`Synth::remove_font`): the channels that used it are
re-pointed, and notes still ringing keep their own reference to the samples until they end.

### Bank numbers

The synthesizer looks for a preset in its fonts from the most recent one down, and subtracts each
font's bank offset from the requested bank, stopping at zero. A request for a pack's bank would
land on bank 0 of any font above it with a higher offset, so the fonts must keep their offsets in
the order of the stack: the most recent the lowest. `Player::add_bank` gives a new bank `100`
minus the number of banks held, and `Player::remove_bank` renumbers the banks added after the one
it removes. A player holds at most 32 banks at once; a score needing more plays the rest with
General MIDI.

## Adding a pack to the app

The bundled packs live in `assets/soundfonts/packs/<id>/`. The manifest and the licence text are
committed; the samples are not (see [`soundfonts.md`](soundfonts.md)).

1. Find samples with an explicit open licence (CC0 or CC-BY) and check it at the source.
2. Create `assets/soundfonts/packs/<id>/` with `pack.json` and `LICENSE.txt`.
3. Add the sample file to `ASSETS` in `scripts/fetch-soundfont.mjs`, as
   `packs/<id>/<file>`, with its checksums. A `.7z` is unpacked by the `7z-wasm` dev dependency.
4. Add the three files to `bundle.resources` in `src-tauri/tauri.conf.json`.
5. Run `pnpm fetch:soundfont`, then `cargo test -p audio-engine`:
   - `the_committed_packs_are_valid_and_their_files_are_fetched_and_bundled` checks steps 2 to 4;
   - `the_declared_key_ranges_match_the_recordings` gives the real key range of the file;
   - `every_bank_plays_its_instrument_over_the_whole_neck` checks it is audible and does not clip.
6. Listen with `cargo run -p audio-engine --release --example render_wav -- out.wav`.
7. Add it to the table in [`soundfonts.md`](soundfonts.md) and to the changelog.

Keep the app light: every bundled pack adds to the download. A large or niche pack is better
offered as a folder users add themselves.

## Limits and next steps

- A pack can only give samples to the sounds of the built-in table; a new sound (a twelve-string
  guitar) is still a change of `score-model`. Letting packs declare new sounds means a string
  `Sound` checked against the loaded table, and a fallback for a score whose pack is missing.
- SFZ files play their regions, round-robin, loops and envelope, but not yet key switches for
  articulations nor release samples; see [`sfz.md`](sfz.md).
- Packs are found at start only: a pack added while the app runs shows up after a restart. The
  `Sounds` panel shows the user's packs folder but does not open it.
