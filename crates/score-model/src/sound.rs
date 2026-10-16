//! The sounds a track can play with. An `Instrument` is a family that decides the strings, the
//! notation and the default tuning; a `Sound` is one way that instrument can sound (a clean or a
//! distorted electric guitar), played with a General MIDI program. See `docs/sounds.md`.

use serde::{Deserialize, Serialize};

use crate::Instrument;

/// A sound of an instrument. In files it is written as a stable id, `"<instrument>/<sound>"`
/// (for instance `"electric-guitar/distortion"`), so that sounds can later come from sound packs
/// without changing the file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sound {
    #[serde(rename = "guitar/nylon")]
    GuitarNylon,
    #[serde(rename = "guitar/steel")]
    GuitarSteel,
    #[serde(rename = "electric-guitar/clean")]
    ElectricGuitarClean,
    #[serde(rename = "electric-guitar/muted")]
    ElectricGuitarMuted,
    #[serde(rename = "electric-guitar/overdrive")]
    ElectricGuitarOverdrive,
    #[serde(rename = "electric-guitar/distortion")]
    ElectricGuitarDistortion,
    #[serde(rename = "bass/finger")]
    BassFinger,
    #[serde(rename = "bass/pick")]
    BassPick,
    #[serde(rename = "bass/slap")]
    BassSlap,
    #[serde(rename = "violin/standard")]
    Violin,
    #[serde(rename = "banjo/standard")]
    Banjo,
    #[serde(rename = "piano/grand")]
    Piano,
    // Drum kits: their `midi_program` is the kit number on the percussion channel, not a melodic
    // program (see `docs/drums.md`).
    #[serde(rename = "drums/standard")]
    DrumsStandard,
    #[serde(rename = "drums/room")]
    DrumsRoom,
    #[serde(rename = "drums/power")]
    DrumsPower,
    #[serde(rename = "drums/electronic")]
    DrumsElectronic,
    #[serde(rename = "drums/jazz")]
    DrumsJazz,
    #[serde(rename = "drums/brush")]
    DrumsBrush,
}

impl Sound {
    /// Every sound, grouped by instrument in the order of `Instrument::ALL`, the default sound of
    /// each instrument first.
    pub const ALL: [Sound; 18] = [
        Sound::GuitarNylon,
        Sound::GuitarSteel,
        Sound::ElectricGuitarClean,
        Sound::ElectricGuitarMuted,
        Sound::ElectricGuitarOverdrive,
        Sound::ElectricGuitarDistortion,
        Sound::BassFinger,
        Sound::BassPick,
        Sound::BassSlap,
        Sound::Violin,
        Sound::Banjo,
        Sound::Piano,
        Sound::DrumsStandard,
        Sound::DrumsRoom,
        Sound::DrumsPower,
        Sound::DrumsElectronic,
        Sound::DrumsJazz,
        Sound::DrumsBrush,
    ];

    /// The id written in files, `"<instrument>/<sound>"`.
    pub fn id(self) -> &'static str {
        match self {
            Sound::GuitarNylon => "guitar/nylon",
            Sound::GuitarSteel => "guitar/steel",
            Sound::ElectricGuitarClean => "electric-guitar/clean",
            Sound::ElectricGuitarMuted => "electric-guitar/muted",
            Sound::ElectricGuitarOverdrive => "electric-guitar/overdrive",
            Sound::ElectricGuitarDistortion => "electric-guitar/distortion",
            Sound::BassFinger => "bass/finger",
            Sound::BassPick => "bass/pick",
            Sound::BassSlap => "bass/slap",
            Sound::Violin => "violin/standard",
            Sound::Banjo => "banjo/standard",
            Sound::Piano => "piano/grand",
            Sound::DrumsStandard => "drums/standard",
            Sound::DrumsRoom => "drums/room",
            Sound::DrumsPower => "drums/power",
            Sound::DrumsElectronic => "drums/electronic",
            Sound::DrumsJazz => "drums/jazz",
            Sound::DrumsBrush => "drums/brush",
        }
    }

    /// The name shown to the user, within its instrument.
    pub fn name(self) -> &'static str {
        match self {
            Sound::GuitarNylon => "Nylon strings",
            Sound::GuitarSteel => "Steel strings",
            Sound::ElectricGuitarClean => "Clean",
            Sound::ElectricGuitarMuted => "Muted",
            Sound::ElectricGuitarOverdrive => "Overdrive",
            Sound::ElectricGuitarDistortion => "Distortion",
            Sound::BassFinger => "Finger",
            Sound::BassPick => "Pick",
            Sound::BassSlap => "Slap",
            Sound::Violin | Sound::Banjo => "Standard",
            Sound::Piano => "Grand piano",
            Sound::DrumsStandard => "Standard kit",
            Sound::DrumsRoom => "Room kit",
            Sound::DrumsPower => "Power kit",
            Sound::DrumsElectronic => "Electronic kit",
            Sound::DrumsJazz => "Jazz kit",
            Sound::DrumsBrush => "Brush kit",
        }
    }

    /// The instrument family the sound belongs to.
    pub fn instrument(self) -> Instrument {
        match self {
            Sound::GuitarNylon | Sound::GuitarSteel => Instrument::Guitar,
            Sound::ElectricGuitarClean
            | Sound::ElectricGuitarMuted
            | Sound::ElectricGuitarOverdrive
            | Sound::ElectricGuitarDistortion => Instrument::ElectricGuitar,
            Sound::BassFinger | Sound::BassPick | Sound::BassSlap => Instrument::Bass,
            Sound::Violin => Instrument::Violin,
            Sound::Banjo => Instrument::Banjo,
            Sound::Piano => Instrument::Piano,
            Sound::DrumsStandard
            | Sound::DrumsRoom
            | Sound::DrumsPower
            | Sound::DrumsElectronic
            | Sound::DrumsJazz
            | Sound::DrumsBrush => Instrument::Drums,
        }
    }

    /// The General MIDI program (0 to 127) that plays the sound. For a drum kit it is the kit
    /// (the program of the percussion channel): 0 standard, 8 room, 16 power, 24 electronic, 32
    /// jazz, 40 brush.
    pub fn midi_program(self) -> u8 {
        match self {
            Sound::GuitarNylon => 24,
            Sound::GuitarSteel => 25,
            Sound::ElectricGuitarClean => 27,
            Sound::ElectricGuitarMuted => 28,
            Sound::ElectricGuitarOverdrive => 29,
            Sound::ElectricGuitarDistortion => 30,
            Sound::BassFinger => 33,
            Sound::BassPick => 34,
            Sound::BassSlap => 36,
            Sound::Violin => 40,
            Sound::Banjo => 105,
            Sound::Piano => 0,
            Sound::DrumsStandard => 0,
            Sound::DrumsRoom => 8,
            Sound::DrumsPower => 16,
            Sound::DrumsElectronic => 24,
            Sound::DrumsJazz => 32,
            Sound::DrumsBrush => 40,
        }
    }

    /// True for a drum kit, which plays on the percussion channel.
    pub fn is_drum_kit(self) -> bool {
        self.instrument() == Instrument::Drums
    }

    /// The drum kit for the program of a percussion channel, when a file names one. The kits of
    /// the General MIDI 2 and GS sets come in groups of eight from the standard one (the TR-808 is
    /// electronic); any other program, or none, gives the standard kit.
    pub fn from_drum_kit(program: Option<u8>) -> Sound {
        match program {
            Some(8..=15) => Sound::DrumsRoom,
            Some(16..=23) => Sound::DrumsPower,
            Some(24..=31) => Sound::DrumsElectronic,
            Some(32..=39) => Sound::DrumsJazz,
            Some(40..=47) => Sound::DrumsBrush,
            _ => Sound::DrumsStandard,
        }
    }

    /// The closest sound to a General MIDI program, when a file names one. Programs of the same
    /// family without a sound of their own go to the nearest one (jazz guitar plays clean,
    /// fretless bass plays finger). Every other program is an instrument without strings here
    /// (keyboards, winds, voice, other strings...): it gets the piano, whose track keeps every
    /// pitch, until those instruments have their own sounds. No program at all gives the nylon
    /// guitar, as files written for a tablature often name none.
    pub fn from_midi_program(program: Option<u8>) -> Sound {
        match program {
            None | Some(24) => Sound::GuitarNylon,
            Some(25) => Sound::GuitarSteel,
            Some(26 | 27 | 31) => Sound::ElectricGuitarClean,
            Some(28) => Sound::ElectricGuitarMuted,
            Some(29) => Sound::ElectricGuitarOverdrive,
            Some(30) => Sound::ElectricGuitarDistortion,
            Some(34) => Sound::BassPick,
            Some(36 | 37) => Sound::BassSlap,
            Some(32..=39) => Sound::BassFinger,
            Some(40) => Sound::Violin,
            Some(105) => Sound::Banjo,
            Some(_) => Sound::Piano,
        }
    }
}

impl Instrument {
    /// The sound a track of this instrument gets when nothing else is said (older files, new
    /// tracks, a change of instrument).
    pub fn default_sound(self) -> Sound {
        match self {
            Instrument::Guitar => Sound::GuitarNylon,
            Instrument::ElectricGuitar => Sound::ElectricGuitarClean,
            Instrument::Bass => Sound::BassFinger,
            Instrument::Violin => Sound::Violin,
            Instrument::Banjo => Sound::Banjo,
            Instrument::Piano => Sound::Piano,
            Instrument::Drums => Sound::DrumsStandard,
        }
    }

    /// The sounds of this instrument, the default one first.
    pub fn sounds(self) -> impl Iterator<Item = Sound> {
        Sound::ALL
            .into_iter()
            .filter(move |s| s.instrument() == self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_survives_a_midi_program_round_trip() {
        for sound in Sound::ALL {
            let back = if sound.is_drum_kit() {
                Sound::from_drum_kit(Some(sound.midi_program()))
            } else {
                Sound::from_midi_program(Some(sound.midi_program()))
            };
            assert_eq!(back, sound, "{sound:?}");
        }
    }

    #[test]
    fn a_kit_without_a_sound_of_its_own_gets_the_closest_one() {
        assert_eq!(Sound::from_drum_kit(Some(25)), Sound::DrumsElectronic);
        assert_eq!(Sound::from_drum_kit(Some(1)), Sound::DrumsStandard);
        assert_eq!(Sound::from_drum_kit(Some(48)), Sound::DrumsStandard);
        assert_eq!(Sound::from_drum_kit(None), Sound::DrumsStandard);
        assert!(!Sound::from_midi_program(Some(0)).is_drum_kit());
    }

    #[test]
    fn a_program_without_a_sound_of_its_own_gets_the_closest_one() {
        assert_eq!(
            Sound::from_midi_program(Some(26)),
            Sound::ElectricGuitarClean
        );
        assert_eq!(Sound::from_midi_program(Some(35)), Sound::BassFinger);
        assert_eq!(Sound::from_midi_program(Some(37)), Sound::BassSlap);
        assert_eq!(Sound::from_midi_program(None), Sound::GuitarNylon);
    }

    #[test]
    fn a_program_without_strings_here_gets_the_piano() {
        for program in [0, 1, 19, 41, 56, 73, 81, 104, 127] {
            assert_eq!(
                Sound::from_midi_program(Some(program)),
                Sound::Piano,
                "{program}"
            );
        }
    }

    #[test]
    fn the_file_id_names_the_instrument_then_the_sound() {
        for sound in Sound::ALL {
            let json = serde_json::to_string(&sound).unwrap();
            assert_eq!(json, format!("\"{}\"", sound.id()));
            let family = serde_json::to_string(&sound.instrument()).unwrap();
            let kebab: String = family
                .trim_matches('"')
                .chars()
                .enumerate()
                .flat_map(|(i, c)| {
                    let dash = (i > 0 && c.is_uppercase()).then_some('-');
                    dash.into_iter().chain(c.to_lowercase())
                })
                .collect();
            assert!(sound.id().starts_with(&format!("{kebab}/")), "{sound:?}");
        }
    }

    #[test]
    fn every_instrument_has_its_default_sound_first() {
        for instrument in Instrument::ALL {
            let sounds: Vec<Sound> = instrument.sounds().collect();
            assert_eq!(sounds.first(), Some(&instrument.default_sound()));
            assert_eq!(instrument.default_sound().instrument(), instrument);
        }
        let all: usize = Instrument::ALL.iter().map(|i| i.sounds().count()).sum();
        assert_eq!(all, Sound::ALL.len());
    }

    #[test]
    fn sound_names_are_distinct_within_an_instrument() {
        for instrument in Instrument::ALL {
            let mut names: Vec<&str> = instrument.sounds().map(Sound::name).collect();
            names.sort_unstable();
            let count = names.len();
            names.dedup();
            assert_eq!(names.len(), count, "{instrument:?}");
        }
    }

    /// `ui/src/lib/instruments.ts` mirrors this table by hand: catch a sound it is missing.
    #[test]
    fn every_sound_is_listed_in_the_ui() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/src/lib/instruments.ts"
        );
        let ui = std::fs::read_to_string(path).expect("ui/src/lib/instruments.ts is missing");
        for sound in Sound::ALL {
            let row = format!(
                "id: '{}', name: '{}', program: {}",
                sound.id(),
                sound.name(),
                sound.midi_program()
            );
            assert!(ui.contains(&row), "instruments.ts lacks {row}");
        }
    }
}
