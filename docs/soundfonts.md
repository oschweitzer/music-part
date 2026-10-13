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

The Tauri build bundles it (`bundle.resources` in `src-tauri/tauri.conf.json`) together with its
licence text, so a release contains both. The build fails if the file is missing.

Audible-path tests in `audio-engine` use `$SOUNDFONT` if set, otherwise that default path, and skip
when neither exists. CI fetches the file so those tests run there.

## Changing the bank

1. Update `COMMIT` and `SHA256` in `scripts/fetch-soundfont.mjs`, and the file names if they differ.
2. Replace the licence text in `assets/soundfonts/` and update this page.
3. Run `cargo test -p audio-engine`: `every_demo_track_is_audible` checks that guitar and bass
   programs exist in the new bank and do not clip.

## Realistic instrument samples

A General MIDI bank is the baseline. Realistic guitar, bass and violin recordings need a sampler
with velocity layers and round-robin, planned in the roadmap. Every sample library added must have
an explicit open licence (CC0 or CC-BY) recorded here.
