# SoundFonts

Playback goes through a General MIDI SoundFont played by `oxisynth` (`audio-engine::Player`).

## Current bank: GeneralUser GS 2.0.3

| Field    | Value                                                                                |
| -------- | ------------------------------------------------------------------------------------ |
| Author   | S. Christian Collins                                                                 |
| Source   | https://github.com/mrbumpy409/GeneralUser-GS, pinned to commit `9704918`             |
| Size     | 30.8 MB, 261 presets and 13 drum kits                                                |
| Licence  | Custom, see [the licence text](../assets/soundfonts/GeneralUser-GS-LICENSE.txt)      |
| Checksum | SHA-256 in `scripts/fetch-soundfont.mjs`                                             |

The licence allows use without restriction, commercial included, and invites redistribution and
modification of the bank and its packaging. It is not an OSI licence.

**Provenance caveat.** The author states that some samples come from other freely available banks
and that he cannot be 100% sure of the origin of every one. Nobody has complained since 2000, but
this matters for a community project. Before a public release, decide whether to keep this bank,
or replace it with one whose samples are all documented (for instance a CC0 or MIT bank).

## Where the file lives

The `.sf2` is **not** committed (31 MB, binary). Fetch it with:

```bash
pnpm fetch:soundfont          # downloads and checks the SHA-256, skips if already valid
pnpm fetch:soundfont --force  # download again
```

It lands in `assets/soundfonts/GeneralUser-GS.sf2` (git-ignored). The licence text is committed
next to it, as the licence asks.

The Tauri build bundles it and the sample banks (`bundle.resources` in
`src-tauri/tauri.conf.json`) together with their licence texts, so a release contains them. The
build fails if a file is missing.

Audible-path tests in `audio-engine` use `$SOUNDFONT` if set, otherwise that default path, and skip
when neither exists. CI fetches the file so those tests run there.

## Changing the bank

1. Update `COMMIT` and `SHA256` in `scripts/fetch-soundfont.mjs`, and the file names if they differ.
2. Replace the licence text in `assets/soundfonts/` and update this page.
3. Run `cargo test -p audio-engine`: `every_demo_track_is_audible` checks that guitar and bass
   programs exist in the new bank and do not clip.

## Sample banks: recordings of real instruments

On top of the General MIDI bank, three sounds (see [`sounds.md`](sounds.md)) play recordings.
Each bank is a small SoundFont of one instrument from the [FreePats](https://freepats.zenvoid.org/)
project, published under **CC0 1.0** (public domain, no credit needed). They are fetched like the base bank, into
`assets/soundfonts/banks/` (git-ignored), with a pinned checksum for the download and for the file
inside it. Licence and sources are in [`banks-LICENSE.txt`](../assets/soundfonts/banks-LICENSE.txt).

| Sound                   | File                  | Recording                                 | Keys covered | Size  |
| ----------------------- | --------------------- | ----------------------------------------- | ------------ | ----- |
| `guitar/nylon`          | `nylon-guitar.sf2`    | Spanish classical guitar                  | 29 to 88     | 19 MB |
| `electric-guitar/clean` | `electric-guitar.sf2` | Fender, clean amp (small version, bridge) | 35 to 86     | 10 MB |
| `bass/finger`           | `electric-bass.sf2`   | Yamaha RBX, played with the fingers       | 26 to 45     | 4 MB  |

A bank replaces the General MIDI program of one sound only: the other sounds of the same
instrument (a steel-string guitar, a distorted electric guitar, a slapped bass) and the violin and
the banjo, which have no free recording yet, keep their General MIDI program.

How it works (`audio-engine::Player::add_bank`):

- Each bank is loaded as another SoundFont, shifted to its own bank number (100, 99, 98...); a
  track picks it with a Bank Select message and program 0. The numbers count down so that the
  last bank added has the lowest: the synthesizer subtracts a font's shift from the requested bank
  and stops at zero, so with the numbers going up, a request for the first bank landed on the
  bank 0 of the last one. The General MIDI sound must not use program 0 for the same reason. Everything the base bank does (pitch wheel, mute, solo, seek)
  works the same.
- **Notes the recording does not cover** are played with the General MIDI sound. This matters for
  the bass, whose recording stops at A2: higher notes go to a second channel of the same track that
  uses the base bank, instead of staying silent. The covered keys are declared in
  `SAMPLE_BANKS` (`audio-engine/src/lib.rs`) and a test checks them against the files.
- A bank that is missing or broken is reported on the console and skipped: that sound plays with
  its General MIDI program.
- What is not there yet: round-robin, per-string recordings, and technique-specific samples
  (hammer-on, palm mute...). Techniques are still the pitch wheel and velocity tricks of
  [`editing.md`](editing.md).

To listen without the app:

```bash
cargo run -p audio-engine --release --example render_wav -- target/demo.wav              # with banks
cargo run -p audio-engine --release --example render_wav -- target/gm.wav --no-banks     # General MIDI only
cargo run -p audio-engine --release --example render_wav -- out.wav song.mid             # a MIDI file
```

`MUSIC_PART_BANKS` points the app at another folder of banks, for development.

### Adding or replacing a bank

1. Find a sound with an explicit open licence (CC0 or CC-BY) and record it in `banks-LICENSE.txt`.
2. Add it to `ASSETS` in `scripts/fetch-soundfont.mjs` (a `.7z` is unpacked by the `7z-wasm` dev
   dependency, so no tool needs installing) and to `bundle.resources` in `tauri.conf.json`.
3. Add it to `SAMPLE_BANKS`; `the_declared_key_ranges_match_the_recordings` gives the real range.
4. Run `cargo test -p audio-engine`, then listen with `render_wav`.
