# Music Part Development Guide

Cross-platform (Windows, macOS, Linux) desktop app to read, play and write scores and tablatures,
string instruments first. Open source, community driven. Light on CPU and RAM is a feature: no GC
runtime, no heavy web stack.

## Project structure

Cargo workspace plus a pnpm workspace: a Tauri 2 shell (`src-tauri`) around a Svelte 5 + Vite front end (`ui`).

```
root/
├── crates/
│   ├── score-model/   # Score > Track > Measure > Voice > Beat > Note, edit operations, validation
│   ├── io-midi/       # MIDI import to Score
│   ├── io-musicxml/   # MusicXML import (.musicxml, .xml, .mxl) to Score
│   ├── io-native/     # The .mpart file: versioned JSON envelope, atomic write
│   └── audio-engine/  # Sequencer (Score -> timed events), SoundFont Player, cpal output
├── src-tauri/         # Tauri shell: commands and events exposing the engines to the UI
│   ├── src/document.rs  # The score being edited: undo history, dirty tracking, file path
│   ├── src/service.rs   # What each command does, free of Tauri (also used by the e2e backend)
│   ├── src/files.rs     # Opening and saving files
│   ├── src/audio.rs     # Audio thread that owns the cpal stream; the app talks to it by message
│   └── examples/headless.rs  # The document backend without window or audio, for the e2e tests
├── scripts/           # fetch-soundfont.mjs, and e2e/ (headless Chrome + the real UI and Rust code)
├── ui/                # Svelte + TypeScript front end
│   ├── public/        # demo-score.json: export of the Rust demo score, checked by a Rust test
│   └── src/lib/       # tab-layout.ts (pure layout), editor.svelte.ts (editing), keymap, selection, playback
└── docs/              # Design notes: soundfonts.md, sounds.md, sound-packs.md, editing.md, musicxml.md
```

The roadmap is in the approved plan: foundations, playback, tab rendering, editing, realistic
samples and technique sounds, then more instruments. Guitar Pro import is not planned.

### Dependency rules

- `score-model` depends on nothing but `serde`. Every other crate depends on it, never the reverse.
- `io-*` crates only convert between a file format and `Score`. No audio, no UI. They must never
  panic or allocate without bound on a bad file: return an error or skip and count what was left
  out. Keep the randomized test in `io-midi` passing when changing an importer.
- Audio hardware stays in `audio-engine/src/output.rs`. Everything else there (sequencer, `Player`)
  must stay testable without a sound card.
- The UI never touches audio. It sends commands to the engine through Tauri and receives the
  position as `playback-position` events. In a browser a simulated transport stands in for them
  (`ui/src/lib/transport.ts`), so every UI feature must work against both.
- Layout lives in pure TypeScript functions (`ui/src/lib/tab-layout.ts`) that return plain geometry;
  Svelte components only draw it. Test layout with vitest, not through the DOM.
- `ui/src/lib/score-types.ts` and `document-api.ts` mirror the serde JSON of `score-model` and of
  the commands by hand. Change both sides together; `pnpm e2e` and the IPC tests in
  `src-tauri/src/lib.rs` catch a mismatch.
- The score lives in Rust (`Document`). The UI never changes a score itself: it sends an `Edit` and
  shows the state that comes back. A new kind of edit is a new `Edit` variant in `score-model`
  with its own tests, a case in `document-api.ts`, and a UI path in `editor.svelte.ts`.
- A score must always pass `Score::validate`. Anything that builds or loads one (import, native
  file, edit) keeps it true; a measure is always exactly full.

## Development

### Commands

```bash
cargo test --workspace                          # Run all tests
cargo fmt --all                                 # Format
cargo clippy --workspace --all-targets -- -D warnings   # Lint, CI fails on any warning
pnpm fetch:soundfont                            # Download the SoundFont and the sound pack samples (assets/soundfonts, not in git)
cargo run -p audio-engine --example play_midi -- song.mid assets/soundfonts/GeneralUser-GS.sf2
cargo run -p audio-engine --release --example render_wav -- out.wav [song.mid] [--no-packs]   # Listen without the app
```

```bash
pnpm install                     # JS dependencies (UI and Tauri CLI)
pnpm dev                         # Run the desktop app (Tauri + Vite dev server on port 1420)
pnpm ui:dev                      # UI alone in a browser, loads ui/public/demo-score.json
pnpm ui:check                    # svelte-check + tsc
pnpm ui:test                     # vitest
pnpm e2e                         # End-to-end: the real UI in headless Chrome against the real document code
cargo run -p music-part-app -- song.mid   # Start the app on a file (needs `pnpm ui:dev` running)
UPDATE_DEMO=1 cargo test -p score-model demo_json   # Regenerate ui/public/demo-score.json
```

Run from the repo root and target a crate with `-p <name>`, never `cd` into a crate. Target a UI
script with `pnpm --filter ui <script>`.

`src-tauri` embeds `ui/dist` at compile time and the build checks that the bundled SoundFont exists,
so on a fresh checkout run `pnpm --filter ui build` and `pnpm fetch:soundfont` once (it also needs
`pnpm install`: the `.7z` samples are unpacked by the `7z-wasm` dev dependency) before
`cargo build`, `cargo clippy` or `cargo test --workspace`. Set `MUSIC_PART_SOUNDFONT` to use
another SoundFont file in the app, and `MUSIC_PART_PACKS` for another folder of sound packs.

The dev server uses port 1420 (fixed, Tauri needs it). Port 5173 is often taken by other projects.

Linux needs system packages for Tauri and ALSA:
`pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev`.

### Naming

- Crates: kebab-case (`score-model`). Rust files and modules: snake_case, as Rust requires.
- Front end files (`ui/`): kebab-case, except Svelte components in PascalCase (`TabView.svelte`).
  Other exceptions: `README.md`, `CLAUDE.md`, `CHANGELOG.md`.
- TypeScript functions and variables: camelCase.

### Rust coding patterns

- Library code never panics on user input (files, SoundFonts, MIDI). Return a `Result` with a
  specific error type. A third-party parser that can panic is wrapped (see `Player::new`).
- `unwrap`/`expect` only in tests, or where the invariant is documented right next to it.
- The real-time audio callback must not allocate, lock or block. Talk to it through channels.
- Prefer enums and small structs with public fields for plain data; keep logic in functions.
- Prefer pure functions. Put public items first and helpers below them.
- Name booleans so they read as a state: `is_complete`, `tied`, `playing`.
- Keep `score-model` units explicit: durations are in 1/128 of a whole note ("ticks").
- Comments explain a non-obvious why or a documented limit, not what the code does. No emojis.
- Doc comments on every public type and on any function whose behaviour is not obvious from its name.
- Language: code, comments, commit messages and docs are written in English.

### TypeScript coding patterns (`ui/`)

- Strict mode, explicit types, no `any`.
- Prefer types over interfaces, functions over classes, single object parameters.
- `import type` for types, regular `import` for values, in separate statements.
- Use `pnpm`, never `npm` or `yarn`.

## Testing

- Three levels: unit tests next to the code; IPC tests (`src-tauri/src/lib.rs`) that send real
  commands through Tauri's mock runtime; and `pnpm e2e`, which drives the UI with clicks and keys.
  Add a scenario to `scripts/e2e/scenarios.mjs` for each user-visible editing feature.
- Tests are self-contained and hardware-free: no sound card, no network, no file outside the repo.
  A test that needs an external asset (the SoundFont) reads its path from an env var, falls back
  to the default fetched location, and skips with a message when neither exists.
- One test verifies one behaviour. Name it after that behaviour
  (`note_crossing_a_barline_is_split_into_tied_notes`).
- Build small scores and MIDI files in the test itself rather than committing binary fixtures.
- When you change a behaviour, update the tests that described the old one.
- A bug fix starts with a test that fails for the bug.

## Git

- Default branch is `main`.
- Until v1 is released, work directly on `main`. Once v1 is out, every change gets its own branch:
  `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `refactor/<topic>`, `chore/<topic>`, merged back
  into `main`. Update this rule when v1 ships.
- Commit only when asked. One logical change per commit, and every commit builds and passes tests.
- Commit messages follow Conventional Commits: `type(scope): summary`, with a lowercase imperative
  summary under 72 characters. Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `chore`,
  `ci`. Scope is the crate or area (`score-model`, `io-midi`, `audio-engine`, `ui`). The body says
  why, not a list of files.
- Mark breaking changes with `!` after the scope and a `BREAKING CHANGE:` line (for instance a
  change to the native score format).
- Commits made with Claude end with this line:
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
- Never force-push `main`, never skip hooks (`--no-verify`), never rewrite published history.
- Stage files by name, not `git add -A`, when the working tree holds unrelated changes.
- Never commit secrets or large binary samples. SoundFonts and sample libraries are fetched by a
  script with a pinned checksum and stay out of git; their licences are recorded in
  `docs/soundfonts.md` and the licence text is committed next to the download location.

## Changelog

Keep `CHANGELOG.md` at the root, in [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/)
format, with Semantic Versioning once releases start.

- After a notable change (feature, fix, removal, behaviour change, change to a file format), add an
  entry under `## [Unreleased]` **in the same commit or branch as the change**.
- Sections: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
- One line per entry, written for a user, naming the crate or area when useful. Not notable:
  refactors with no visible effect, test-only changes, CI tweaks, formatting.
- At release time, rename `Unreleased` to the version and date, and open a fresh `Unreleased`.

## Conventions

- **Formats**: the native score format is JSON via `serde`. Adding a field must keep older files
  loadable (`#[serde(default)]`), and a change that cannot is a breaking change in the changelog.
- **Licences**: code is PolyForm Noncommercial 1.0.0 (source-available, commercial use needs the author's
  permission; see `LICENSE`). A new dependency must allow being combined with it: permissive licences
  (MIT, Apache-2.0, MPL-2.0) are fine, GPL and AGPL are not. `oxisynth` is LGPL-2.1: it stays under
  its own licence and must remain a replaceable library, never copied into our code. Audio assets
  need an explicit open licence recorded next to them. `7z-wasm` (LGPL) is a build-time tool for
  `pnpm fetch:soundfont` and is never shipped.
- **CI**: GitHub Actions runs fmt, clippy and tests on Linux, Windows and macOS. Keep all three green.
- **Markdown tables**: keep columns aligned, padding each cell so the `|` separators line up in
  the raw source.
- **Documentation**: when a design decision is not obvious from the code (the score model, the
  audio threading, the licence of a sample bank), write it in `docs/` in the same change.
