//! Loads the sample files of sound packs only when a score needs them. The manifests are read at
//! start ([`crate::pack::discover`]); a pack's file is read when a track of the score uses one of
//! its sounds, and its bank is removed from the player when no track uses it any more. See
//! `docs/sound-packs.md`.

use std::fs::File;

use score_model::{Score, Sound};

use crate::pack::{PackError, PackSound, SoundPack};
use crate::player::{BankFont, Player, PlayerError, MAX_BANKS};

/// A change to the sample banks of a [`Player`], worked out by [`PackBanks::update`] and applied
/// with [`apply`]. It carries the samples already read, so it can be sent to the thread that owns
/// the player.
pub enum BankChange {
    /// Play `sound` with these samples over `keys`.
    Add {
        sound: Sound,
        font: BankFont,
        keys: std::ops::RangeInclusive<u8>,
    },
    /// Play `sound` with its General MIDI program again.
    Remove(Sound),
}

/// Applies a change to `player`. Returns the samples of a removed bank, for the caller to free:
/// on the real-time audio thread they are sent elsewhere to be dropped.
pub fn apply(player: &mut Player, change: BankChange) -> Option<BankFont> {
    match change {
        // `PackBanks` never holds more than `MAX_BANKS`, so this cannot fail; were it to, the
        // sound would just keep its General MIDI program.
        BankChange::Add { sound, font, keys } => {
            let _ = player.add_bank_font(sound, font, keys);
            None
        }
        BankChange::Remove(sound) => player.remove_bank(sound),
    }
}

/// A sound of a pack whose sample file could not be used. It is not tried again: the sound plays
/// with the previous pack that has it, or with its General MIDI program.
#[derive(Debug, Clone, PartialEq)]
pub struct PackFailure {
    /// Id of the pack.
    pub pack: String,
    pub sound: Sound,
    pub error: PackError,
}

/// The packs found at start, and which of their sample files a player holds. Lives next to the
/// player's owner, away from the real-time thread: [`PackBanks::update`] reads files.
pub struct PackBanks {
    packs: Vec<SoundPack>,
    /// The sounds whose bank is in the player, with the pack it came from.
    loaded: Vec<(Sound, String)>,
    failures: Vec<PackFailure>,
}

impl PackBanks {
    /// `packs` in order of priority, lowest first, as [`crate::pack::discover`] returns them.
    pub fn new(packs: Vec<SoundPack>) -> Self {
        Self {
            packs,
            loaded: Vec::new(),
            failures: Vec::new(),
        }
    }

    pub fn packs(&self) -> &[SoundPack] {
        &self.packs
    }

    /// The sample files that could not be used so far.
    pub fn failures(&self) -> &[PackFailure] {
        &self.failures
    }

    /// The pack that plays `sound`: the last one listing it whose file has not failed.
    pub fn source(&self, sound: Sound) -> Option<(&SoundPack, &PackSound)> {
        self.packs.iter().rev().find_map(|pack| {
            let listed = pack.manifest.sounds.iter().find(|s| s.sound == sound)?;
            let has_failed = self
                .failures
                .iter()
                .any(|f| f.sound == sound && f.pack == pack.manifest.id);
            (!has_failed).then_some((pack, listed))
        })
    }

    /// The sounds whose bank the player holds, in the order they were added.
    pub fn loaded(&self) -> impl Iterator<Item = Sound> + '_ {
        self.loaded.iter().map(|(sound, _)| *sound)
    }

    /// The changes that make the player hold the banks of the sounds `score` uses, and no other.
    /// Reads the sample files of the sounds that just came into use, which takes a while: call it
    /// away from the real-time audio thread, and apply the changes before loading the score.
    pub fn update(&mut self, score: &Score) -> Vec<BankChange> {
        let used: Vec<Sound> = score.tracks.iter().map(|t| t.sound).collect();
        let mut changes = Vec::new();
        self.loaded.retain(|(sound, _)| {
            let is_used = used.contains(sound);
            if !is_used {
                changes.push(BankChange::Remove(*sound));
            }
            is_used
        });
        for sound in used {
            if self.loaded.len() >= usize::from(MAX_BANKS)
                || self.loaded.iter().any(|(s, _)| *s == sound)
            {
                continue;
            }
            // A file that fails falls back on the previous pack with the sound, if any.
            while let Some((pack, listed)) = self.source(sound) {
                let id = pack.manifest.id.clone();
                match read_bank(pack, listed) {
                    Ok(font) => {
                        changes.push(BankChange::Add {
                            sound,
                            font,
                            keys: listed.keys(),
                        });
                        self.loaded.push((sound, id));
                        break;
                    }
                    Err(error) => self.failures.push(PackFailure {
                        pack: id,
                        sound,
                        error,
                    }),
                }
            }
        }
        changes
    }
}

fn read_bank(pack: &SoundPack, sound: &PackSound) -> Result<BankFont, PackError> {
    File::open(pack.file(sound))
        .map_err(|e| PlayerError::SoundFont(e.to_string()))
        .and_then(|mut file| BankFont::read(&mut file))
        .map_err(|error| PackError {
            dir: pack.dir.clone(),
            reason: format!("{}: {error}", sound.file),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::parse_manifest;
    use crate::test_fonts::tiny_soundfont;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A fresh folder under `target/`, so tests stay inside the repository.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-banks")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A pack `id` in `root` playing `sounds`, each with a tiny SoundFont whose preset is named
    /// `"<id> <sound>"`, unless `broken` (then the file is junk).
    fn pack(root: &Path, id: &str, sounds: &[Sound], broken: bool) -> SoundPack {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let mut entries = Vec::new();
        for (i, sound) in sounds.iter().enumerate() {
            let file = format!("s{i}.sf2");
            let bytes = if broken {
                vec![7; 64]
            } else {
                tiny_soundfont(&preset(id, *sound))
            };
            fs::write(dir.join(&file), bytes).unwrap();
            entries.push(format!(
                r#"{{ "sound": "{}", "file": "{file}", "firstKey": 0, "lastKey": 127 }}"#,
                sound.id()
            ));
        }
        let manifest = format!(
            r#"{{ "format": 1, "id": "{id}", "name": "{id}", "author": "Test", "licence": "CC0-1.0",
                "sounds": [{}] }}"#,
            entries.join(", ")
        );
        SoundPack {
            dir,
            manifest: parse_manifest(&manifest).unwrap(),
        }
    }

    fn preset(id: &str, sound: Sound) -> String {
        // A preset name holds 19 characters at most.
        format!("{id} {}", sound.id()).chars().take(19).collect()
    }

    fn score_with(sounds: &[Sound]) -> Score {
        let mut score = crate::player::tests::one_note_score();
        let track = score.tracks[0].clone();
        score.tracks = sounds
            .iter()
            .map(|sound| {
                let mut track = track.clone();
                track.instrument = sound.instrument();
                track.sound = *sound;
                track
            })
            .collect();
        score
    }

    /// A player holding only the base SoundFont, or `None` when it has not been fetched.
    fn player() -> Option<Player> {
        let Some(base) = crate::player::tests::test_soundfont() else {
            eprintln!("no SoundFont available, test skipped");
            return None;
        };
        Some(Player::new(&mut File::open(base).unwrap(), 44_100).unwrap())
    }

    /// Loads `score` the way the app does: bank changes first, then the score. Returns the name of
    /// the preset each track plays.
    fn load(player: &mut Player, banks: &mut PackBanks, score: &Score) -> Vec<Option<String>> {
        for change in banks.update(score) {
            apply(player, change);
        }
        player.load_score(score);
        (0..score.tracks.len())
            .map(|track| player.preset_of_track(track))
            .collect()
    }

    fn kinds(changes: &[BankChange]) -> Vec<(char, Sound)> {
        changes
            .iter()
            .map(|change| match change {
                BankChange::Add { sound, .. } => ('+', *sound),
                BankChange::Remove(sound) => ('-', *sound),
            })
            .collect()
    }

    #[test]
    fn only_the_sounds_a_score_uses_are_read() {
        let root = temp_dir("only-used");
        let mut banks = PackBanks::new(vec![pack(
            &root,
            "two",
            &[Sound::GuitarNylon, Sound::BassFinger],
            false,
        )]);

        let changes = banks.update(&score_with(&[Sound::GuitarNylon, Sound::GuitarSteel]));

        assert_eq!(kinds(&changes), [('+', Sound::GuitarNylon)]);
        assert_eq!(banks.loaded().collect::<Vec<_>>(), [Sound::GuitarNylon]);
    }

    #[test]
    fn a_bank_is_read_once_and_removed_when_no_track_uses_it() {
        let root = temp_dir("once");
        let mut banks = PackBanks::new(vec![pack(&root, "nylon", &[Sound::GuitarNylon], false)]);
        let nylon = score_with(&[Sound::GuitarNylon, Sound::GuitarNylon]);

        assert_eq!(kinds(&banks.update(&nylon)), [('+', Sound::GuitarNylon)]);
        assert!(banks.update(&nylon).is_empty());
        assert_eq!(
            kinds(&banks.update(&score_with(&[Sound::GuitarSteel]))),
            [('-', Sound::GuitarNylon)]
        );
        assert_eq!(banks.loaded().count(), 0);
    }

    #[test]
    fn a_broken_file_is_reported_once_and_the_previous_pack_plays_the_sound() {
        let root = temp_dir("broken");
        let mut banks = PackBanks::new(vec![
            pack(&root, "good", &[Sound::GuitarNylon], false),
            pack(&root, "broken", &[Sound::GuitarNylon], true),
        ]);
        let score = score_with(&[Sound::GuitarNylon]);

        assert_eq!(kinds(&banks.update(&score)), [('+', Sound::GuitarNylon)]);
        assert_eq!(banks.failures().len(), 1);
        assert_eq!(banks.failures()[0].pack, "broken");
        assert!(banks.failures()[0].error.reason.starts_with("s0.sf2"));
        assert_eq!(
            banks.source(Sound::GuitarNylon).unwrap().0.manifest.id,
            "good"
        );

        banks.update(&score_with(&[Sound::GuitarSteel]));
        banks.update(&score);
        assert_eq!(banks.failures().len(), 1, "a broken file is not read again");
    }

    #[test]
    fn a_missing_file_leaves_the_sound_on_general_midi() {
        let root = temp_dir("missing");
        let nylon = pack(&root, "nylon", &[Sound::GuitarNylon], false);
        fs::remove_file(nylon.dir.join("s0.sf2")).unwrap();
        let mut banks = PackBanks::new(vec![nylon]);

        assert!(banks.update(&score_with(&[Sound::GuitarNylon])).is_empty());
        assert_eq!(banks.failures().len(), 1);
        assert!(banks.source(Sound::GuitarNylon).is_none());
    }

    #[test]
    fn each_track_plays_its_own_bank_when_banks_come_and_go_in_any_order() {
        let Some(mut player) = player() else { return };
        let root = temp_dir("any-order");
        let sounds = [
            Sound::GuitarNylon,
            Sound::BassFinger,
            Sound::ElectricGuitarClean,
            Sound::Banjo,
        ];
        let mut banks = PackBanks::new(vec![pack(&root, "all", &sounds, false)]);
        let own = |sound: Sound| Some(preset("all", sound));
        let base = load(
            &mut self::player().unwrap(),
            &mut PackBanks::new(Vec::new()),
            &score_with(&[Sound::GuitarSteel]),
        );

        // Each step adds some banks and removes others, so they are numbered in every order.
        let steps: [&[Sound]; 5] = [
            &[Sound::GuitarNylon, Sound::BassFinger],
            &[Sound::BassFinger, Sound::ElectricGuitarClean],
            &[Sound::Banjo, Sound::ElectricGuitarClean, Sound::GuitarNylon],
            &[Sound::GuitarNylon, Sound::GuitarSteel],
            &sounds,
        ];
        for step in steps {
            let presets = load(&mut player, &mut banks, &score_with(step));
            for (sound, preset) in step.iter().zip(&presets) {
                if *sound == Sound::GuitarSteel {
                    assert_eq!(
                        preset, &base[0],
                        "a sound without a pack plays General MIDI"
                    );
                } else {
                    assert_eq!(preset, &own(*sound), "{step:?}");
                }
            }
            let mut held: Vec<Sound> = player.bank_sounds().collect();
            let mut wanted: Vec<Sound> = step
                .iter()
                .copied()
                .filter(|s| *s != Sound::GuitarSteel)
                .collect();
            held.sort_by_key(|s| s.id());
            wanted.sort_by_key(|s| s.id());
            assert_eq!(held, wanted, "the player holds only the banks in use");
        }
    }

    #[test]
    fn removing_a_bank_brings_back_the_sound_it_replaced() {
        let Some(mut player) = player() else { return };
        let root = temp_dir("replaced");
        let first = pack(&root, "first", &[Sound::GuitarNylon], false);
        let second = pack(&root, "second", &[Sound::GuitarNylon], false);
        let score = score_with(&[Sound::GuitarNylon]);
        player.load_score(&score);
        let general_midi = player.preset_of_track(0);
        let font = |pack: &SoundPack| read_bank(pack, &pack.manifest.sounds[0]).unwrap();

        player
            .add_bank_font(Sound::GuitarNylon, font(&first), 0..=127)
            .unwrap();
        player
            .add_bank_font(Sound::GuitarNylon, font(&second), 0..=127)
            .unwrap();
        player.load_score(&score);
        assert_eq!(
            player.preset_of_track(0),
            Some(preset("second", Sound::GuitarNylon))
        );

        assert!(player.remove_bank(Sound::GuitarNylon).is_some());
        player.load_score(&score);
        assert_eq!(
            player.preset_of_track(0),
            Some(preset("first", Sound::GuitarNylon))
        );

        assert!(player.remove_bank(Sound::GuitarNylon).is_some());
        player.load_score(&score);
        assert_eq!(player.preset_of_track(0), general_midi);
        assert!(player.remove_bank(Sound::GuitarNylon).is_none());
    }

    #[test]
    fn a_bank_loaded_later_plays_its_recordings() {
        let (Some(mut plain), Some(mut sampled)) = (player(), player()) else {
            return;
        };
        let root = temp_dir("audible");
        let mut banks = PackBanks::new(vec![pack(&root, "nylon", &[Sound::GuitarNylon], false)]);
        let render = |player: &mut Player, banks: &mut PackBanks| {
            load(player, banks, &score_with(&[Sound::GuitarSteel]));
            load(player, banks, &score_with(&[Sound::GuitarNylon]));
            player.play();
            let mut buf = vec![0.0f32; 2 * 11_025];
            player.render(&mut buf);
            buf
        };

        let with_bank = render(&mut sampled, &mut banks);
        let without = render(&mut plain, &mut PackBanks::new(Vec::new()));

        assert!(
            with_bank.iter().any(|s| s.abs() > 0.01),
            "the bank is heard"
        );
        assert_ne!(with_bank, without);
    }
}
