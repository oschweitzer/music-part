//! Loads the sample files of sound packs only when a score needs them. The manifests are read at
//! start ([`crate::pack::discover`]); a pack's file is read when a track of the score uses one of
//! its sounds, and its bank is removed from the player when no track uses it any more. See
//! `docs/sound-packs.md`.

use std::fs::File;

use score_model::{Score, Sound};

use crate::pack::{PackError, PackNote, PackSound, SoundPack};
use crate::player::{BankFont, Player, PlayerError, RemovedBank, MAX_BANKS};
use crate::sfz::{self, SfzInstrument};

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
    /// Play `sound` with this SFZ instrument over `keys`.
    AddSfz {
        sound: Sound,
        instrument: SfzInstrument,
        keys: std::ops::RangeInclusive<u8>,
    },
    /// Play `sound` with its General MIDI program again.
    Remove(Sound),
}

/// Applies a change to `player`. Returns the samples of a removed bank, for the caller to free:
/// on the real-time audio thread they are sent elsewhere to be dropped.
pub fn apply(player: &mut Player, change: BankChange) -> Option<RemovedBank> {
    match change {
        // `PackBanks` never holds more than `MAX_BANKS`, so this cannot fail; were it to, the
        // sound would just keep its General MIDI program.
        BankChange::Add { sound, font, keys } => {
            let _ = player.add_bank_font(sound, font, keys);
            None
        }
        BankChange::AddSfz {
            sound,
            instrument,
            keys,
        } => {
            let _ = player.add_sfz(sound, instrument, keys);
            None
        }
        BankChange::Remove(sound) => player.remove_latest_bank(sound),
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
    /// The sounds whose bank is in the player, with the pack it came from and the bytes of
    /// decoded SFZ samples it holds (0 for a SoundFont).
    loaded: Vec<(Sound, String, u64)>,
    failures: Vec<PackFailure>,
    /// What the SFZ files loaded so far left out.
    notes: Vec<PackNote>,
    /// Most bytes of decoded SFZ samples one pack may hold at once.
    sfz_budget: u64,
}

impl PackBanks {
    /// `packs` in order of priority, lowest first, as [`crate::pack::discover`] returns them.
    pub fn new(packs: Vec<SoundPack>) -> Self {
        Self {
            packs,
            loaded: Vec::new(),
            failures: Vec::new(),
            notes: Vec::new(),
            sfz_budget: sfz::MAX_PACK_SAMPLE_BYTES,
        }
    }

    pub fn packs(&self) -> &[SoundPack] {
        &self.packs
    }

    /// The sample files that could not be used so far.
    pub fn failures(&self) -> &[PackFailure] {
        &self.failures
    }

    /// What each SFZ file loaded so far left out (opcodes not supported, samples missing), in
    /// the order they were loaded.
    pub fn notes(&self) -> &[PackNote] {
        &self.notes
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
        self.loaded.iter().map(|(sound, _, _)| *sound)
    }

    /// The changes that make the player hold the banks of the sounds `score` uses, and no other.
    /// Reads the sample files of the sounds that just came into use, which takes a while: call it
    /// away from the real-time audio thread, and apply the changes before loading the score.
    pub fn update(&mut self, score: &Score) -> Vec<BankChange> {
        let used: Vec<Sound> = score.tracks.iter().map(|t| t.sound).collect();
        let mut changes = Vec::new();
        self.loaded.retain(|(sound, _, _)| {
            let is_used = used.contains(sound);
            if !is_used {
                changes.push(BankChange::Remove(*sound));
            }
            is_used
        });
        for sound in used {
            if self.loaded.len() >= usize::from(MAX_BANKS)
                || self.loaded.iter().any(|(s, _, _)| *s == sound)
            {
                continue;
            }
            // A file that fails falls back on the previous pack with the sound, if any.
            while let Some((pack, listed)) = self.source(sound) {
                let id = pack.manifest.id.clone();
                let read = if listed.is_sfz() {
                    // The SFZ samples of a pack loaded at once share its memory cap.
                    let used: u64 = self
                        .loaded
                        .iter()
                        .filter(|(_, pack, _)| *pack == id)
                        .map(|(_, _, bytes)| bytes)
                        .sum();
                    let budget = self.sfz_budget.saturating_sub(used);
                    read_sfz(pack, listed, budget).map(|(instrument, note)| {
                        let bytes = instrument.sample_bytes();
                        let change = BankChange::AddSfz {
                            sound,
                            instrument,
                            keys: listed.keys(),
                        };
                        (change, bytes, Some(note))
                    })
                } else {
                    read_bank(pack, listed).map(|font| {
                        let change = BankChange::Add {
                            sound,
                            font,
                            keys: listed.keys(),
                        };
                        (change, 0, None)
                    })
                };
                match read {
                    Ok((change, bytes, note)) => {
                        self.notes.extend(note);
                        changes.push(change);
                        self.loaded.push((sound, id, bytes));
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

/// Loads the SFZ instrument of `sound` with at most `budget` bytes of decoded samples, and says
/// what it left out.
fn read_sfz(
    pack: &SoundPack,
    sound: &PackSound,
    mut budget: u64,
) -> Result<(SfzInstrument, PackNote), PackError> {
    match sfz::load(&pack.file(sound), &pack.dir, &mut budget) {
        Ok((instrument, report)) => Ok((
            instrument,
            PackNote {
                dir: pack.dir.clone(),
                note: format!("{}: {report}", sound.file),
            },
        )),
        Err(error) => Err(PackError {
            dir: pack.dir.clone(),
            reason: format!("{}: {error}", sound.file),
        }),
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
                BankChange::AddSfz { sound, .. } => ('~', *sound),
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

    /// A pack `id` in `root` whose `sounds` are SFZ instruments, each one region over every key
    /// playing a tenth of a second of sine (17 640 bytes once decoded). With `broken`, the `.sfz`
    /// names a sample that does not exist.
    fn sfz_pack(root: &Path, id: &str, sounds: &[Sound], broken: bool) -> SoundPack {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let mut entries = Vec::new();
        for (i, sound) in sounds.iter().enumerate() {
            let file = format!("s{i}.sfz");
            let sample = if broken { "missing.wav" } else { "sine.wav" };
            fs::write(
                dir.join(&file),
                format!("<region> sample={sample} pitch_keycenter=69 sw_last=36"),
            )
            .unwrap();
            entries.push(format!(
                r#"{{ "sound": "{}", "file": "{file}", "firstKey": 0, "lastKey": 127 }}"#,
                sound.id()
            ));
        }
        fs::write(
            dir.join("sine.wav"),
            crate::sfz::tests::sine_wav(440.0, 0.5, 0.1),
        )
        .unwrap();
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

    #[test]
    fn an_sfz_instrument_is_read_when_used_and_removed_when_not() {
        let root = temp_dir("sfz-on-demand");
        let mut banks = PackBanks::new(vec![sfz_pack(
            &root,
            "sfz",
            &[Sound::GuitarNylon, Sound::BassFinger],
            false,
        )]);
        let nylon = score_with(&[Sound::GuitarNylon]);

        assert_eq!(kinds(&banks.update(&nylon)), [('~', Sound::GuitarNylon)]);
        assert!(banks.update(&nylon).is_empty());
        assert_eq!(banks.notes().len(), 1);
        assert!(
            banks.notes()[0].note.contains("1 × sw_last"),
            "{}",
            banks.notes()[0]
        );
        assert_eq!(
            kinds(&banks.update(&score_with(&[Sound::BassFinger]))),
            [('-', Sound::GuitarNylon), ('~', Sound::BassFinger)]
        );
        assert_eq!(banks.loaded().collect::<Vec<_>>(), [Sound::BassFinger]);
    }

    #[test]
    fn the_sfz_samples_a_pack_holds_at_once_share_its_memory_cap() {
        let root = temp_dir("sfz-budget");
        let mut banks = PackBanks::new(vec![sfz_pack(
            &root,
            "sfz",
            &[Sound::GuitarNylon, Sound::BassFinger],
            false,
        )]);
        // Room for one instrument's samples, not two.
        banks.sfz_budget = 4410 * 4 + 100;

        let both = banks.update(&score_with(&[Sound::GuitarNylon, Sound::BassFinger]));
        assert_eq!(kinds(&both), [('~', Sound::GuitarNylon)]);
        assert_eq!(banks.failures().len(), 1);
        assert_eq!(banks.failures()[0].sound, Sound::BassFinger);
        assert!(
            banks.failures()[0]
                .error
                .reason
                .contains("over the pack memory limit"),
            "{}",
            banks.failures()[0].error
        );
    }

    #[test]
    fn a_refused_sfz_is_a_failure_and_the_previous_pack_plays_the_sound() {
        let root = temp_dir("sfz-broken");
        let mut banks = PackBanks::new(vec![
            pack(&root, "good", &[Sound::GuitarNylon], false),
            sfz_pack(&root, "broken", &[Sound::GuitarNylon], true),
        ]);

        let changes = banks.update(&score_with(&[Sound::GuitarNylon]));

        assert_eq!(kinds(&changes), [('+', Sound::GuitarNylon)]);
        assert_eq!(banks.failures().len(), 1);
        assert_eq!(banks.failures()[0].pack, "broken");
        assert!(
            banks.failures()[0].error.reason.starts_with("s0.sfz"),
            "{}",
            banks.failures()[0].error
        );
    }

    #[test]
    fn the_latest_bank_of_a_sound_plays_it_soundfont_or_sfz() {
        let Some(mut player) = player() else { return };
        let root = temp_dir("sfz-latest");
        let font_pack = pack(&root, "font", &[Sound::GuitarNylon], false);
        let sfz_pack = sfz_pack(&root, "sfz", &[Sound::GuitarNylon], false);
        let font = || read_bank(&font_pack, &font_pack.manifest.sounds[0]).unwrap();
        let instrument = || {
            read_sfz(&sfz_pack, &sfz_pack.manifest.sounds[0], u64::MAX)
                .unwrap()
                .0
        };
        let score = score_with(&[Sound::GuitarNylon]);
        player.load_score(&score);
        let general_midi = player.preset_of_track(0);
        let from_font = Some(preset("font", Sound::GuitarNylon));
        // A track the SFZ instrument plays keeps the General MIDI program on its channel, for
        // the notes it does not cover; the SoundFont bank is not on it.
        let playing = |player: &mut Player| {
            player.load_score(&score);
            player.preset_of_track(0)
        };

        player
            .add_bank_font(Sound::GuitarNylon, font(), 0..=127)
            .unwrap();
        player
            .add_sfz(Sound::GuitarNylon, instrument(), 0..=127)
            .unwrap();
        assert_eq!(
            playing(&mut player),
            general_midi,
            "the SFZ added last plays"
        );
        assert!(matches!(
            player.remove_latest_bank(Sound::GuitarNylon),
            Some(RemovedBank::Sfz(_))
        ));
        assert_eq!(playing(&mut player), from_font, "the SoundFont is back");

        player
            .add_sfz(Sound::GuitarNylon, instrument(), 0..=127)
            .unwrap();
        player
            .add_bank_font(Sound::GuitarNylon, font(), 0..=127)
            .unwrap();
        assert_eq!(
            playing(&mut player),
            from_font,
            "the SoundFont added last plays"
        );
        assert!(matches!(
            player.remove_latest_bank(Sound::GuitarNylon),
            Some(RemovedBank::Font(_))
        ));
        assert_eq!(playing(&mut player), general_midi);
        assert_eq!(player.bank_sounds().count(), 2);
        // The SoundFont added first is still there, under the SFZ, which is what is heard.
        let render = |player: &mut Player| {
            player.load_score(&score);
            player.play();
            let mut buf = vec![0.0f32; 2 * 4410];
            player.render(&mut buf);
            buf
        };
        let with_sfz = render(&mut player);
        assert!(with_sfz.iter().any(|s| s.abs() > 0.01), "the SFZ is heard");
        assert!(matches!(
            player.remove_latest_bank(Sound::GuitarNylon),
            Some(RemovedBank::Sfz(_))
        ));
        assert_ne!(render(&mut player), with_sfz);
        assert!(player.remove_latest_bank(Sound::GuitarNylon).is_some());
        assert!(player.remove_latest_bank(Sound::GuitarNylon).is_none());
        assert_eq!(playing(&mut player), general_midi);
    }
}
