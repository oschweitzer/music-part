//! Sound packs: a folder holding a `pack.json` manifest and the sample files it lists. A pack
//! gives recordings to one or more sounds of the built-in table (`score_model::Sound`); the app
//! finds packs at start and plays every note a pack does not cover with the General MIDI sound.
//! See `docs/sound-packs.md`.

use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{sfz, Player, PlayerError};
use score_model::Sound;

/// Name of the manifest file at the root of a pack folder.
pub const MANIFEST: &str = "pack.json";
/// Version of the manifest format this build reads.
pub const FORMAT: u32 = 1;
/// A manifest is a few hundred bytes; anything far larger is not one.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// What a `pack.json` says: who made the pack, under which licence, and which sounds it plays.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    /// Version of the manifest format, [`FORMAT`] for this build.
    pub format: u32,
    /// Stable id in kebab-case, such as `freepats-nylon-guitar`. A pack in a later folder with the
    /// same id replaces the earlier one (a user's copy over the bundled one).
    pub id: String,
    pub name: String,
    pub author: String,
    /// SPDX identifier of the licence of the samples, such as `CC0-1.0` or `CC-BY-4.0`.
    pub licence: String,
    /// Where the samples come from.
    #[serde(default)]
    pub source: Option<String>,
    pub sounds: Vec<PackSound>,
}

/// One sound a pack plays: the sample file, and the keys its recordings cover.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackSound {
    /// A sound of the built-in table, by its id (`"guitar/nylon"`).
    pub sound: Sound,
    /// A SoundFont (`.sf2`) or an SFZ instrument (`.sfz`, see `docs/sfz.md`) in the pack folder,
    /// by its plain file name.
    pub file: String,
    /// First and last MIDI key the recordings cover. Notes outside are played with the General
    /// MIDI sound.
    pub first_key: u8,
    pub last_key: u8,
}

impl PackSound {
    pub fn keys(&self) -> RangeInclusive<u8> {
        self.first_key..=self.last_key
    }

    /// Whether the file is an SFZ instrument rather than a SoundFont.
    pub fn is_sfz(&self) -> bool {
        self.file.to_ascii_lowercase().ends_with(".sfz")
    }
}

/// A pack found on disk: its folder and its checked manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundPack {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// Why a pack folder was left out.
#[derive(Debug, Clone, PartialEq)]
pub struct PackError {
    pub dir: PathBuf,
    pub reason: String,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sound pack {} not used: {}",
            self.dir.display(),
            self.reason
        )
    }
}

impl std::error::Error for PackError {}

/// Something worth knowing about a pack that was loaded: what its SFZ files left out.
#[derive(Debug, Clone, PartialEq)]
pub struct PackNote {
    pub dir: PathBuf,
    pub note: String,
}

impl fmt::Display for PackNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sound pack {}: {}", self.dir.display(), self.note)
    }
}

impl SoundPack {
    /// Reads and checks the manifest of the pack in `dir`. The sample files are not opened here.
    pub fn read(dir: &Path) -> Result<Self, PackError> {
        let fail = |reason: String| PackError {
            dir: dir.to_path_buf(),
            reason,
        };
        let mut text = String::new();
        File::open(dir.join(MANIFEST))
            .map_err(|e| fail(format!("cannot open {MANIFEST}: {e}")))?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|e| fail(format!("cannot read {MANIFEST}: {e}")))?;
        if text.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(fail(format!("{MANIFEST} is too large")));
        }
        let manifest = parse_manifest(&text).map_err(fail)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    /// Path of the sample file of `sound`, inside the pack folder.
    pub fn file(&self, sound: &PackSound) -> PathBuf {
        self.dir.join(&sound.file)
    }
}

/// Parses and checks a manifest.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    // Read the version first, so a pack made for a newer app says so instead of failing on a
    // field this build does not know.
    #[derive(Deserialize)]
    struct Version {
        format: u32,
    }
    let version: Version =
        serde_json::from_str(text).map_err(|e| format!("invalid {MANIFEST}: {e}"))?;
    if version.format != FORMAT {
        return Err(format!(
            "manifest format {} is not supported (this version reads {FORMAT})",
            version.format
        ));
    }
    let manifest: Manifest =
        serde_json::from_str(text).map_err(|e| format!("invalid {MANIFEST}: {e}"))?;
    check(&manifest)?;
    Ok(manifest)
}

/// Finds the packs in each of `roots`, every subfolder holding a `pack.json` being one. Roots come
/// in order of priority, lowest first: a pack whose id was already found is replaced. A root that
/// does not exist is skipped silently; a pack that cannot be read is returned as an error.
pub fn discover(roots: &[PathBuf]) -> (Vec<SoundPack>, Vec<PackError>) {
    let mut packs: Vec<SoundPack> = Vec::new();
    let mut errors = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.join(MANIFEST).is_file())
            .collect();
        // The order of read_dir depends on the file system; the last pack wins a sound, so keep
        // it the same everywhere.
        dirs.sort();
        for dir in dirs {
            match SoundPack::read(&dir) {
                Ok(pack) => {
                    packs.retain(|p| p.manifest.id != pack.manifest.id);
                    packs.push(pack);
                }
                Err(error) => errors.push(error),
            }
        }
    }
    (packs, errors)
}

/// Adds the sample files of `packs` to `player`, in order: when two packs play the same sound the
/// later one is heard. A file that is missing or broken is returned as an error and skipped; that
/// sound then plays with its General MIDI program.
pub fn add_packs(player: &mut Player, packs: &[SoundPack]) -> Vec<PackError> {
    add_packs_with_notes(player, packs).0
}

/// [`add_packs`], also returning what each SFZ file of the packs left out (opcodes not supported,
/// samples missing), for the console. The decoded samples of a pack's SFZ files share
/// [`sfz::MAX_PACK_SAMPLE_BYTES`].
pub fn add_packs_with_notes(
    player: &mut Player,
    packs: &[SoundPack],
) -> (Vec<PackError>, Vec<PackNote>) {
    let mut errors = Vec::new();
    let mut notes = Vec::new();
    for pack in packs {
        let mut budget = sfz::MAX_PACK_SAMPLE_BYTES;
        for sound in &pack.manifest.sounds {
            let path = pack.file(sound);
            if sound.is_sfz() {
                match sfz::load(&path, &pack.dir, &mut budget) {
                    Ok((instrument, report)) => {
                        match player.add_sfz(sound.sound, instrument, sound.keys()) {
                            Ok(()) => notes.push(PackNote {
                                dir: pack.dir.clone(),
                                note: format!("{}: {report}", sound.file),
                            }),
                            Err(error) => errors.push(PackError {
                                dir: pack.dir.clone(),
                                reason: format!("{}: {error}", sound.file),
                            }),
                        }
                    }
                    Err(error) => errors.push(PackError {
                        dir: pack.dir.clone(),
                        reason: format!("{}: {error}", sound.file),
                    }),
                }
                continue;
            }
            let added = File::open(&path)
                .map_err(|e| PlayerError::SoundFont(e.to_string()))
                .and_then(|mut file| player.add_bank(sound.sound, &mut file, sound.keys()));
            if let Err(error) = added {
                errors.push(PackError {
                    dir: pack.dir.clone(),
                    reason: format!("{}: {error}", sound.file),
                });
            }
        }
    }
    (errors, notes)
}

fn check(manifest: &Manifest) -> Result<(), String> {
    let is_kebab = |id: &str| {
        !id.is_empty()
            && !id.starts_with('-')
            && !id.ends_with('-')
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    if !is_kebab(&manifest.id) {
        return Err(format!(
            "id {:?} must be lowercase letters, digits and dashes",
            manifest.id
        ));
    }
    for (field, value) in [
        ("name", &manifest.name),
        ("author", &manifest.author),
        ("licence", &manifest.licence),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} is empty"));
        }
    }
    if manifest.sounds.is_empty() {
        return Err("the pack has no sounds".into());
    }
    for (index, sound) in manifest.sounds.iter().enumerate() {
        if manifest.sounds[..index]
            .iter()
            .any(|s| s.sound == sound.sound)
        {
            return Err(format!("{} is listed twice", sound.sound.id()));
        }
        // A plain name keeps every file the pack opens inside its own folder.
        let is_plain_name = !sound.file.is_empty()
            && !sound.file.contains(['/', '\\', ':'])
            && sound.file != "."
            && sound.file != "..";
        if !is_plain_name {
            return Err(format!(
                "file {:?} must be a file name in the pack folder",
                sound.file
            ));
        }
        let lower = sound.file.to_ascii_lowercase();
        if !lower.ends_with(".sf2") && !lower.ends_with(".sfz") {
            return Err(format!(
                "{}: only SoundFont (.sf2) and SFZ (.sfz) files are supported",
                sound.file
            ));
        }
        if sound.first_key > sound.last_key || sound.last_key > 127 {
            return Err(format!(
                "{}: keys {} to {} are not a range of MIDI keys",
                sound.sound.id(),
                sound.first_key,
                sound.last_key
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NYLON: &str = r#"{
        "format": 1,
        "id": "test-nylon",
        "name": "Test nylon guitar",
        "author": "Someone",
        "licence": "CC0-1.0",
        "sounds": [
            { "sound": "guitar/nylon", "file": "nylon.sf2", "firstKey": 29, "lastKey": 88 }
        ]
    }"#;

    /// A fresh empty folder under `target/`, so tests stay inside the repository.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-packs")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_pack(root: &Path, folder: &str, manifest: &str) -> PathBuf {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), manifest).unwrap();
        dir
    }

    fn with(field: &str, value: &str) -> String {
        NYLON.replace(field, value)
    }

    #[test]
    fn a_manifest_names_its_sounds_and_their_keys() {
        let manifest = parse_manifest(NYLON).unwrap();
        assert_eq!(manifest.id, "test-nylon");
        assert_eq!(manifest.source, None);
        assert_eq!(manifest.sounds[0].sound, Sound::GuitarNylon);
        assert_eq!(manifest.sounds[0].keys(), 29..=88);
    }

    #[test]
    fn a_manifest_of_a_newer_format_says_so() {
        let newer = with(r#""format": 1"#, r#""format": 2, "sampleFormat": "sfz""#);
        let error = parse_manifest(&newer).unwrap_err();
        assert!(error.contains("format 2 is not supported"), "{error}");
    }

    #[test]
    fn a_file_outside_the_pack_folder_is_refused() {
        for file in ["../other.sf2", "sub/nylon.sf2", "C:\\nylon.sf2", ".."] {
            let manifest = with(r#""file": "nylon.sf2""#, &format!(r#""file": {file:?}"#));
            assert!(parse_manifest(&manifest).is_err(), "{file} was accepted");
        }
    }

    #[test]
    fn a_manifest_with_a_wrong_field_is_refused() {
        let cases = [
            with(r#""guitar/nylon""#, r#""guitar/banjo""#),
            with(r#""firstKey": 29"#, r#""firstKey": 90"#),
            with(r#""lastKey": 88"#, r#""lastKey": 128"#),
            with(r#""nylon.sf2""#, r#""nylon.wav""#),
            with(r#""test-nylon""#, r#""Test Nylon""#),
            with(r#""CC0-1.0""#, r#""  ""#),
            with(r#""author""#, r#""auteur""#),
            NYLON.replace(
                r#"{ "sound": "guitar/nylon", "file": "nylon.sf2", "firstKey": 29, "lastKey": 88 }"#,
                "",
            ),
        ];
        for case in &cases {
            assert!(parse_manifest(case).is_err(), "accepted: {case}");
        }
    }

    #[test]
    fn a_sound_listed_twice_in_a_pack_is_refused() {
        let line =
            r#"{ "sound": "guitar/nylon", "file": "nylon.sf2", "firstKey": 29, "lastKey": 88 }"#;
        let twice = NYLON.replace(line, &format!("{line}, {line}"));
        assert!(parse_manifest(&twice).unwrap_err().contains("listed twice"));
    }

    #[test]
    fn discovery_finds_the_folders_with_a_manifest_and_reports_broken_ones() {
        let root = temp_dir("discovery");
        write_pack(&root, "b-good", NYLON);
        write_pack(&root, "a-broken", "{ not json");
        fs::create_dir_all(root.join("not-a-pack")).unwrap();
        fs::write(root.join("stray-file.txt"), "hello").unwrap();

        let missing = root.join("does-not-exist");
        let (packs, errors) = discover(&[missing, root.clone()]);

        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].dir, root.join("b-good"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].dir, root.join("a-broken"));
    }

    #[test]
    fn a_pack_in_a_later_folder_replaces_the_one_with_the_same_id() {
        let bundled = temp_dir("bundled");
        let user = temp_dir("user");
        write_pack(&bundled, "nylon", NYLON);
        write_pack(&user, "my-nylon", &with("Someone", "Me"));

        let (packs, errors) = discover(&[bundled, user.clone()]);

        assert!(errors.is_empty());
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].manifest.author, "Me");
        assert_eq!(packs[0].dir, user.join("my-nylon"));
    }

    #[test]
    fn a_huge_manifest_is_refused_without_reading_it_all() {
        let root = temp_dir("huge");
        let padding = " ".repeat(MAX_MANIFEST_BYTES as usize);
        let dir = write_pack(&root, "huge", &format!("{padding}{NYLON}"));
        assert!(SoundPack::read(&dir)
            .unwrap_err()
            .reason
            .contains("too large"));
    }

    #[test]
    fn a_missing_sample_file_is_reported_and_skipped() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let root = temp_dir("missing-file");
        let pack = SoundPack::read(&write_pack(&root, "nylon", NYLON)).unwrap();
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();

        let errors = add_packs(&mut player, &[pack]);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].reason.starts_with("nylon.sf2"), "{}", errors[0]);
    }

    #[test]
    fn a_pack_plays_its_sound_and_leaves_the_others_alone() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        // Any SoundFont will do as the pack's samples: its first preset (a piano for the base
        // one) plays instead of the nylon guitar.
        let root = temp_dir("plays");
        let dir = write_pack(&root, "nylon", NYLON);
        fs::copy(&base, dir.join("nylon.sf2")).unwrap();
        let (packs, errors) = discover(&[root]);
        assert!(errors.is_empty(), "{errors:?}");

        let render = |sound: Sound, with_pack: bool| {
            let mut player = Player::new(&mut File::open(&base).unwrap(), 44_100).unwrap();
            if with_pack {
                assert!(add_packs(&mut player, &packs).is_empty());
            }
            let mut score = crate::player::tests::one_note_score();
            score.tracks[0].sound = sound;
            player.load_score(&score);
            player.play();
            let mut buf = vec![0.0f32; 2 * 22_050];
            player.render(&mut buf);
            buf
        };

        assert_ne!(
            render(Sound::GuitarNylon, true),
            render(Sound::GuitarNylon, false)
        );
        assert_eq!(
            render(Sound::GuitarSteel, true),
            render(Sound::GuitarSteel, false)
        );
    }

    /// A pack whose nylon guitar is an SFZ instrument: a sine at 440 Hz recorded at key 69 for the
    /// keys 60 to 70 (the open high E string is 64), declared for the keys `first` to `last`.
    fn sfz_pack(name: &str, sfz: &str, first: u8, last: u8) -> Vec<SoundPack> {
        let root = temp_dir(name);
        let manifest = NYLON
            .replace("nylon.sf2", "nylon.sfz")
            .replace("29", &first.to_string())
            .replace("88", &last.to_string());
        let dir = write_pack(&root, "nylon", &manifest);
        crate::sfz::tests::write_sfz(
            &dir,
            sfz,
            &[("a4.wav", crate::sfz::tests::sine_wav(440.0, 0.8, 2.0))],
        );
        fs::rename(dir.join("test.sfz"), dir.join("nylon.sfz")).unwrap();
        let (packs, errors) = discover(&[root]);
        assert!(errors.is_empty(), "{errors:?}");
        packs
    }

    const SFZ: &str = "<region> sample=a4.wav lokey=60 hikey=70 pitch_keycenter=69 \
                       ampeg_release=0.2 loop_mode=one_shot";

    #[test]
    fn a_manifest_may_name_an_sfz_file() {
        let manifest = parse_manifest(&with("nylon.sf2", "Nylon.SFZ")).unwrap();
        assert!(manifest.sounds[0].is_sfz());
        assert!(!parse_manifest(NYLON).unwrap().sounds[0].is_sfz());
    }

    /// Renders one second of the one-note score (open high E, key 64) with `packs`, through
    /// `prepare` before playing.
    fn render_sfz(
        base: &Path,
        packs: &[SoundPack],
        score: &score_model::Score,
        prepare: impl FnOnce(&mut Player),
    ) -> Vec<f32> {
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        let (errors, _) = add_packs_with_notes(&mut player, packs);
        assert!(errors.is_empty(), "{errors:?}");
        player.load_score(score);
        prepare(&mut player);
        player.play();
        let mut buf = vec![0.0f32; 2 * 44_100];
        player.render(&mut buf);
        buf
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn an_sfz_pack_reports_what_it_left_out() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let packs = sfz_pack("sfz-notes", &format!("{SFZ} sw_last=36"), 29, 88);
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        let (errors, notes) = add_packs_with_notes(&mut player, &packs);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].note.starts_with("nylon.sfz: 1 regions"),
            "{}",
            notes[0]
        );
        assert!(notes[0].note.contains("1 × sw_last"), "{}", notes[0]);
    }

    #[test]
    fn an_sfz_pack_plays_its_samples_instead_of_general_midi() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = crate::player::tests::one_note_score();
        let packs = sfz_pack("sfz-plays", SFZ, 29, 88);
        let with_sfz = render_sfz(&base, &packs, &score, |_| {});
        let general_midi = render_sfz(&base, &[], &score, |_| {});
        assert_ne!(with_sfz, general_midi);
        let peak = peak(&with_sfz);
        assert!(peak > 0.05 && peak <= 1.0, "peak {peak}");

        // The sine of the recording, transposed from A4 down to E4: 329.6 Hz. Count the rising
        // zero crossings of the left channel over the first half second.
        let left: Vec<f32> = with_sfz.iter().step_by(2).take(22_050).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((160..=170).contains(&crossings), "{crossings}");
    }

    #[test]
    fn notes_the_sfz_does_not_cover_play_the_general_midi_program() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let score = crate::player::tests::one_note_score();
        let general_midi = render_sfz(&base, &[], &score, |_| {});
        // Key 64 outside the keys the manifest declares.
        let outside_manifest = sfz_pack("sfz-outside-manifest", SFZ, 29, 63);
        // Key 64 inside them, but no region answers it.
        let no_region = sfz_pack(
            "sfz-no-region",
            "<region> sample=a4.wav lokey=65 hikey=70 pitch_keycenter=69",
            29,
            88,
        );
        for packs in [outside_manifest, no_region] {
            assert_eq!(render_sfz(&base, &packs, &score, |_| {}), general_midi);
        }
    }

    #[test]
    fn an_sfz_track_that_is_not_audible_stays_silent() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let packs = sfz_pack("sfz-muted", SFZ, 29, 88);
        let score = crate::player::tests::one_note_score();
        let buf = render_sfz(&base, &packs, &score, |player| {
            player.set_audible(vec![false])
        });
        assert!(peak(&buf) < 1e-3, "peak {}", peak(&buf));
    }

    #[test]
    fn seeking_and_stopping_cut_the_sfz_voices() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        let packs = sfz_pack("sfz-seek", SFZ, 29, 88);
        let mut player = Player::new(&mut File::open(base).unwrap(), 44_100).unwrap();
        add_packs(&mut player, &packs);
        player.load_score(&crate::player::tests::one_note_score());
        player.play();
        let mut buf = vec![0.0f32; 2 * 4410];
        player.render(&mut buf);
        assert!(peak(&buf) > 0.05);

        // Past the attack of the only note: nothing may sound any more.
        player.seek(64.0);
        player.render(&mut buf);
        player.render(&mut buf);
        assert!(peak(&buf) < 1e-3, "peak {}", peak(&buf));
    }

    #[test]
    fn every_technique_plays_through_an_sfz_instrument() {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return;
        };
        use score_model::{Beat, Duration, Note, NoteValue, Technique};
        let packs = sfz_pack("sfz-techniques", SFZ, 29, 88);
        // Two half notes, E4 then F#4, the first one with the technique.
        let score = |technique| {
            let mut score = crate::player::tests::one_note_score();
            let half = |fret, technique| Beat {
                duration: Duration {
                    value: NoteValue::Half,
                    dots: 0,
                },
                notes: vec![Note {
                    string: 1,
                    fret,
                    technique,
                    tied: false,
                }],
            };
            score.tracks[0].measures[0].voices[0].beats = vec![half(0, technique), half(2, None)];
            score
        };
        let plain = render_sfz(&base, &packs, &score(None), |_| {});
        for technique in [
            Technique::PalmMute,
            Technique::Harmonic,
            Technique::HammerOn,
            Technique::PullOff,
            Technique::Bend,
            Technique::Slide,
        ] {
            let buf = render_sfz(&base, &packs, &score(Some(technique)), |_| {});
            let peak = peak(&buf);
            assert!(peak > 0.01 && peak <= 1.0, "{technique:?}: peak {peak}");
            assert!(buf.iter().all(|s| s.is_finite()));
            assert_ne!(buf, plain, "{technique:?} sounds like plain notes");
        }
    }

    /// The packs committed in `assets/soundfonts/packs`, whose samples are fetched separately.
    #[test]
    fn the_committed_packs_are_valid_and_their_files_are_fetched_and_bundled() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/soundfonts");
        let (packs, errors) = discover(&[assets.join("packs")]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!packs.is_empty());

        let repo = assets.join("../..");
        let fetch = fs::read_to_string(repo.join("scripts/fetch-soundfont.mjs")).unwrap();
        let bundle = fs::read_to_string(repo.join("src-tauri/tauri.conf.json")).unwrap();
        for pack in &packs {
            let folder = pack.dir.file_name().unwrap().to_str().unwrap();
            assert_eq!(
                folder, pack.manifest.id,
                "a pack folder is named after its id"
            );
            assert!(
                pack.dir.join("LICENSE.txt").is_file(),
                "{folder} has no LICENSE.txt"
            );
            for name in ["pack.json", "LICENSE.txt"] {
                let path = format!("packs/{folder}/{name}");
                assert!(bundle.contains(&path), "{path} is not bundled");
            }
            for sound in &pack.manifest.sounds {
                let path = format!("packs/{folder}/{}", sound.file);
                assert!(fetch.contains(&path), "{path} is not fetched");
                assert!(bundle.contains(&path), "{path} is not bundled");
            }
        }
    }
}
