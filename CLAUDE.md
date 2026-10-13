# Music Part Development Guide

Cross-platform (Windows, macOS, Linux) desktop app to read, play and write scores and tablatures,
string instruments first. Open source, community driven. Light on CPU and RAM is a feature: no GC
runtime, no heavy web stack.

## Project structure

Cargo workspace with a Tauri + Svelte front end (not created yet).

```
root/
├── crates/
│   ├── score-model/   # Score > Track > Measure > Voice > Beat > Note. Pure data, no I/O
│   ├── io-midi/       # MIDI import to Score
│   └── audio-engine/  # Sequencer (Score -> timed events), SoundFont Player, cpal output
├── src-tauri/         # (planned) Tauri commands exposing the engine to the UI
├── ui/                # (planned) Svelte + TypeScript front end, score and tab rendering
└── docs/              # (planned) Design notes, one file per feature
```

The roadmap is in the approved plan: foundations, playback, tab rendering, editing, realistic
samples, then Guitar Pro import and more instruments.

### Dependency rules

- `score-model` depends on nothing but `serde`. Every other crate depends on it, never the reverse.
- `io-*` crates only convert between a file format and `Score`. No audio, no UI.
- Audio hardware stays in `audio-engine/src/output.rs`. Everything else there (sequencer, `Player`)
  must stay testable without a sound card.
- The UI never touches audio. It sends commands to the engine through Tauri.

## Development

### Commands

```bash
cargo test --workspace                          # Run all tests
cargo fmt --all                                 # Format
cargo clippy --workspace --all-targets -- -D warnings   # Lint, CI fails on any warning
SOUNDFONT=/usr/share/sounds/sf2/TimGM6mb.sf2 cargo test -p audio-engine   # Includes the audible-path test
cargo run -p audio-engine --example play_midi -- song.mid font.sf2        # Listen to a MIDI file
```

Run from the repo root and target a crate with `-p <name>`, never `cd` into a crate.

Linux needs system packages for Tauri and ALSA:
`pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev`.

### Naming

- Crates: kebab-case (`score-model`). Rust files and modules: snake_case, as Rust requires.
- Front end files (`ui/`): kebab-case. Exceptions: `README.md`, `CLAUDE.md`, `CHANGELOG.md`.
- Svelte components: PascalCase. TypeScript functions and variables: camelCase.

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

- Tests are self-contained and hardware-free: no sound card, no network, no file outside the repo.
  A test that needs an external asset (a SoundFont) reads its path from an env var and skips with a
  message when it is unset.
- One test verifies one behaviour. Name it after that behaviour
  (`note_crossing_a_barline_is_split_into_tied_notes`).
- Build small scores and MIDI files in the test itself rather than committing binary fixtures.
- When you change a behaviour, update the tests that described the old one.
- A bug fix starts with a test that fails for the bug.

## Git

- Default branch is `main`. Work on a branch per change: `feat/<topic>`, `fix/<topic>`,
  `docs/<topic>`, `refactor/<topic>`, `chore/<topic>`.
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
- Never commit secrets, local SoundFonts, or large binary samples. Sample libraries are fetched by
  a script and stay out of git; their licences are listed in `docs/` (CC0 or CC-BY only).

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
- **Licences**: code is MPL-2.0. A new dependency must be compatible with it. Audio assets need an
  explicit open licence recorded next to them.
- **CI**: GitHub Actions runs fmt, clippy and tests on Linux, Windows and macOS. Keep all three green.
- **Markdown tables**: keep columns aligned, padding each cell so the `|` separators line up in
  the raw source.
- **Documentation**: when a design decision is not obvious from the code (the score model, the
  audio threading, the licence of a sample bank), write it in `docs/` in the same change.
