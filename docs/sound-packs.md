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
| `sounds[].file`     | A SoundFont (`.sf2`) in the pack folder, by its plain file name.                |
| `sounds[].firstKey` | First MIDI key the recordings cover.                                            |
| `sounds[].lastKey`  | Last MIDI key the recordings cover.                                             |

The SoundFont's first preset is played. Notes outside `firstKey` to `lastKey` are played with the
General MIDI program of the sound rather than left silent: the bass recording stops at A2, the
higher notes come from the base SoundFont. A manifest with an unknown field, a file outside its
folder or a key range that is not one is refused as a whole.

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
console and skipped: its sounds keep their General MIDI program, and the app starts anyway. The
console also lists every pack loaded, with its licence.

Code: `audio-engine/src/pack.rs` (manifest, discovery, loading into the `Player`) and
`src-tauri/src/audio.rs` (which folders, at start).

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
- Only SoundFont 2 files. SFZ (round-robin, key switches for articulations) would be a second
  value of the file type, played by another engine behind the same `Player` interface; see the
  follow-ups in [`sounds.md`](sounds.md).
- Every pack found is loaded at start, used by the score or not. With three small packs this
  costs little; with many, load a pack only when a track of the score uses one of its sounds.
- The UI does not list the packs or their licences yet; the console does.
