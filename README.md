# Music Part

A fast, light, cross-platform (Windows, macOS, Linux) desktop app to read, play and write scores and
tablatures. String instruments first. Built with Rust (score model, audio) and Tauri 2 with a Svelte
front end, so it stays small and quick, with no Java-style runtime.

Status: early development. Not released yet.

## What works today

- Tablature display with a playback cursor, and playback through a SoundFont.
- Writing: select a note, type frets, durations, dots, ties and techniques, measures, tempo, tracks
  and tuning. Undo and redo.
- Files: the native `.mpart` format (save and open), and MIDI import.

See [`docs/editing.md`](docs/editing.md) for the keyboard shortcuts and the editing rules.

## Run it

You need Rust, Node.js and pnpm. On Linux, also the system packages listed in
[`CLAUDE.md`](CLAUDE.md).

```bash
pnpm install
pnpm fetch:soundfont   # downloads the SoundFont, which is not stored in git
pnpm dev               # opens the desktop app
```

Tests: `cargo test --workspace`, `pnpm ui:test`, and `pnpm e2e` (the real UI in headless Chrome).
Project layout, conventions and the commit rules are in [`CLAUDE.md`](CLAUDE.md).

## Licence

[PolyForm Noncommercial 1.0.0](LICENSE): you can use, study, change and share the code for any
non-commercial purpose. Selling it or using it commercially needs the author's permission. This is a
source-available licence, not an OSI open source one. The SoundFont keeps its own licence, see
[`docs/soundfonts.md`](docs/soundfonts.md).
