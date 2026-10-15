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

use crate::{Player, PlayerError};
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
    /// A SoundFont (`.sf2`) in the pack folder, by its plain file name.
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
    let mut errors = Vec::new();
    for pack in packs {
        for sound in &pack.manifest.sounds {
            let path = pack.file(sound);
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
    errors
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
        if !sound.file.to_ascii_lowercase().ends_with(".sf2") {
            return Err(format!(
                "{}: only SoundFont (.sf2) files are supported",
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
            with(r#""nylon.sf2""#, r#""nylon.sfz""#),
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
