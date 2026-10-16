//! The sounds a track can play with. An `Instrument` is a family that decides the strings, the
//! notation and the default tuning; a `Sound` is one way that instrument can sound (a clean or a
//! distorted electric guitar). The built-in sounds are a fixed table played with General MIDI
//! programs; a sound pack can declare more, which this crate knows only by their id. See
//! `docs/sounds.md` and `docs/sound-packs.md`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Instrument;

/// Longest sound id, in bytes. Ids are kept inline so a `Sound` stays `Copy` and can be handled on
/// the real-time audio thread without allocating.
pub const MAX_SOUND_ID_LEN: usize = 62;

/// A sound of an instrument, known by a stable id `"<instrument>/<sound>"` in kebab-case (for
/// instance `"electric-guitar/distortion"`), which is how files and the UI write it. The prefix
/// always names an existing instrument family ([`Instrument::sound_prefix`]). Either a sound of the
/// built-in table ([`BUILT_IN_SOUNDS`], with a constant each, such as [`Sound::GUITAR_NYLON`]) or
/// one a sound pack declares: this crate cannot tell a pack's sound from a sound of a pack that is
/// not installed, so any well-formed id of a melodic instrument is accepted. Drum kits are only
/// the built-in ones (see `docs/drums.md`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sound {
    instrument: Instrument,
    len: u8,
    bytes: [u8; MAX_SOUND_ID_LEN],
}

/// A sound of the built-in table: its name within its instrument and the General MIDI program that
/// plays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInSound {
    pub sound: Sound,
    /// The name shown to the user, within its instrument.
    pub name: &'static str,
    /// The General MIDI program (0 to 127) that plays the sound. For a drum kit it is the kit (the
    /// program of the percussion channel): 0 standard, 8 room, 16 power, 24 electronic, 32 jazz,
    /// 40 brush.
    pub midi_program: u8,
}

/// Every built-in sound, grouped by instrument in the order of `Instrument::ALL`, the default sound
/// of each instrument first. Ids are never renamed once released: files name them.
#[rustfmt::skip]
pub const BUILT_IN_SOUNDS: [BuiltInSound; 32] = [
    BuiltInSound { sound: Sound::GUITAR_NYLON, name: "Nylon strings", midi_program: 24 },
    BuiltInSound { sound: Sound::GUITAR_STEEL, name: "Steel strings", midi_program: 25 },
    BuiltInSound { sound: Sound::ELECTRIC_GUITAR_CLEAN, name: "Clean", midi_program: 27 },
    BuiltInSound { sound: Sound::ELECTRIC_GUITAR_MUTED, name: "Muted", midi_program: 28 },
    BuiltInSound { sound: Sound::ELECTRIC_GUITAR_OVERDRIVE, name: "Overdrive", midi_program: 29 },
    BuiltInSound { sound: Sound::ELECTRIC_GUITAR_DISTORTION, name: "Distortion", midi_program: 30 },
    BuiltInSound { sound: Sound::BASS_FINGER, name: "Finger", midi_program: 33 },
    BuiltInSound { sound: Sound::BASS_PICK, name: "Pick", midi_program: 34 },
    BuiltInSound { sound: Sound::BASS_SLAP, name: "Slap", midi_program: 36 },
    BuiltInSound { sound: Sound::VIOLIN, name: "Standard", midi_program: 40 },
    BuiltInSound { sound: Sound::BANJO, name: "Standard", midi_program: 105 },
    BuiltInSound { sound: Sound::PIANO, name: "Grand piano", midi_program: 0 },
    BuiltInSound { sound: Sound::PIANO_BRIGHT, name: "Bright piano", midi_program: 1 },
    BuiltInSound { sound: Sound::PIANO_ELECTRIC, name: "Electric piano", midi_program: 4 },
    BuiltInSound { sound: Sound::PIANO_ELECTRIC_FM, name: "FM electric piano", midi_program: 5 },
    BuiltInSound { sound: Sound::ORGAN_DRAWBAR, name: "Drawbar", midi_program: 16 },
    BuiltInSound { sound: Sound::ORGAN_ROCK, name: "Rock", midi_program: 18 },
    BuiltInSound { sound: Sound::ORGAN_CHURCH, name: "Church", midi_program: 19 },
    BuiltInSound { sound: Sound::ORGAN_REED, name: "Reed", midi_program: 20 },
    BuiltInSound { sound: Sound::SYNTH_SQUARE_LEAD, name: "Square lead", midi_program: 80 },
    BuiltInSound { sound: Sound::SYNTH_SAW_LEAD, name: "Saw lead", midi_program: 81 },
    BuiltInSound { sound: Sound::SYNTH_WARM_PAD, name: "Warm pad", midi_program: 89 },
    BuiltInSound { sound: Sound::SYNTH_CHOIR_PAD, name: "Choir pad", midi_program: 91 },
    BuiltInSound { sound: Sound::VOICE_AAHS, name: "Choir aahs", midi_program: 52 },
    BuiltInSound { sound: Sound::VOICE_OOHS, name: "Voice oohs", midi_program: 53 },
    BuiltInSound { sound: Sound::VOICE_SYNTH, name: "Synth voice", midi_program: 54 },
    BuiltInSound { sound: Sound::DRUMS_STANDARD, name: "Standard kit", midi_program: 0 },
    BuiltInSound { sound: Sound::DRUMS_ROOM, name: "Room kit", midi_program: 8 },
    BuiltInSound { sound: Sound::DRUMS_POWER, name: "Power kit", midi_program: 16 },
    BuiltInSound { sound: Sound::DRUMS_ELECTRONIC, name: "Electronic kit", midi_program: 24 },
    BuiltInSound { sound: Sound::DRUMS_JAZZ, name: "Jazz kit", midi_program: 32 },
    BuiltInSound { sound: Sound::DRUMS_BRUSH, name: "Brush kit", midi_program: 40 },
];

/// Why a sound id was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundIdError {
    /// Not `"<instrument>/<sound>"` with a sound name in lowercase letters, digits and dashes.
    Malformed(String),
    /// Longer than [`MAX_SOUND_ID_LEN`] bytes.
    TooLong,
    /// The part before the slash is not an instrument family.
    UnknownInstrument(String),
    /// A drum kit that is not one of the built-in kits: packs cannot declare kits.
    UnknownDrumKit(String),
}

impl fmt::Display for SoundIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(id) => write!(
                f,
                "sound id {id:?} must be \"<instrument>/<sound>\" in lowercase letters, digits and dashes"
            ),
            Self::TooLong => write!(f, "a sound id has at most {MAX_SOUND_ID_LEN} characters"),
            Self::UnknownInstrument(id) => {
                write!(f, "sound id {id:?} does not start with an instrument")
            }
            Self::UnknownDrumKit(id) => {
                write!(f, "{id:?} is not a drum kit: kits are only the built-in ones")
            }
        }
    }
}

impl std::error::Error for SoundIdError {}

impl Sound {
    pub const GUITAR_NYLON: Sound = Sound::built_in("guitar/nylon", Instrument::Guitar);
    pub const GUITAR_STEEL: Sound = Sound::built_in("guitar/steel", Instrument::Guitar);
    pub const ELECTRIC_GUITAR_CLEAN: Sound =
        Sound::built_in("electric-guitar/clean", Instrument::ElectricGuitar);
    pub const ELECTRIC_GUITAR_MUTED: Sound =
        Sound::built_in("electric-guitar/muted", Instrument::ElectricGuitar);
    pub const ELECTRIC_GUITAR_OVERDRIVE: Sound =
        Sound::built_in("electric-guitar/overdrive", Instrument::ElectricGuitar);
    pub const ELECTRIC_GUITAR_DISTORTION: Sound =
        Sound::built_in("electric-guitar/distortion", Instrument::ElectricGuitar);
    pub const BASS_FINGER: Sound = Sound::built_in("bass/finger", Instrument::Bass);
    pub const BASS_PICK: Sound = Sound::built_in("bass/pick", Instrument::Bass);
    pub const BASS_SLAP: Sound = Sound::built_in("bass/slap", Instrument::Bass);
    pub const VIOLIN: Sound = Sound::built_in("violin/standard", Instrument::Violin);
    pub const BANJO: Sound = Sound::built_in("banjo/standard", Instrument::Banjo);
    pub const PIANO: Sound = Sound::built_in("piano/grand", Instrument::Piano);
    pub const PIANO_BRIGHT: Sound = Sound::built_in("piano/bright", Instrument::Piano);
    pub const PIANO_ELECTRIC: Sound = Sound::built_in("piano/electric", Instrument::Piano);
    pub const PIANO_ELECTRIC_FM: Sound = Sound::built_in("piano/electric-fm", Instrument::Piano);
    pub const ORGAN_DRAWBAR: Sound = Sound::built_in("organ/drawbar", Instrument::Organ);
    pub const ORGAN_ROCK: Sound = Sound::built_in("organ/rock", Instrument::Organ);
    pub const ORGAN_CHURCH: Sound = Sound::built_in("organ/church", Instrument::Organ);
    pub const ORGAN_REED: Sound = Sound::built_in("organ/reed", Instrument::Organ);
    pub const SYNTH_SQUARE_LEAD: Sound = Sound::built_in("synth/square-lead", Instrument::Synth);
    pub const SYNTH_SAW_LEAD: Sound = Sound::built_in("synth/saw-lead", Instrument::Synth);
    pub const SYNTH_WARM_PAD: Sound = Sound::built_in("synth/warm-pad", Instrument::Synth);
    pub const SYNTH_CHOIR_PAD: Sound = Sound::built_in("synth/choir-pad", Instrument::Synth);
    pub const VOICE_AAHS: Sound = Sound::built_in("voice/aahs", Instrument::Voice);
    pub const VOICE_OOHS: Sound = Sound::built_in("voice/oohs", Instrument::Voice);
    pub const VOICE_SYNTH: Sound = Sound::built_in("voice/synth", Instrument::Voice);
    pub const DRUMS_STANDARD: Sound = Sound::built_in("drums/standard", Instrument::Drums);
    pub const DRUMS_ROOM: Sound = Sound::built_in("drums/room", Instrument::Drums);
    pub const DRUMS_POWER: Sound = Sound::built_in("drums/power", Instrument::Drums);
    pub const DRUMS_ELECTRONIC: Sound = Sound::built_in("drums/electronic", Instrument::Drums);
    pub const DRUMS_JAZZ: Sound = Sound::built_in("drums/jazz", Instrument::Drums);
    pub const DRUMS_BRUSH: Sound = Sound::built_in("drums/brush", Instrument::Drums);

    /// Every built-in sound, in the order of [`BUILT_IN_SOUNDS`].
    pub const ALL: [Sound; 32] = {
        let mut all = [Sound::GUITAR_NYLON; 32];
        let mut i = 0;
        while i < all.len() {
            all[i] = BUILT_IN_SOUNDS[i].sound;
            i += 1;
        }
        all
    };

    /// A sound from an id already checked, or one of the literal ids above (a test checks they
    /// all parse back to the same sound, so they are well formed and short enough).
    const fn built_in(id: &str, instrument: Instrument) -> Sound {
        let mut bytes = [0u8; MAX_SOUND_ID_LEN];
        let source = id.as_bytes();
        let mut i = 0;
        while i < source.len() && i < MAX_SOUND_ID_LEN {
            bytes[i] = source[i];
            i += 1;
        }
        Sound {
            instrument,
            len: i as u8,
            bytes,
        }
    }

    /// Reads a sound id: a built-in sound, or a sound a pack may declare. Refuses an id that is not
    /// `"<instrument>/<sound>"` in kebab-case, is too long, or names a drum kit that is not built in.
    pub fn parse(id: &str) -> Result<Sound, SoundIdError> {
        if id.len() > MAX_SOUND_ID_LEN {
            return Err(SoundIdError::TooLong);
        }
        let malformed = || SoundIdError::Malformed(id.to_string());
        let (prefix, name) = id.split_once('/').ok_or_else(malformed)?;
        let is_kebab = !name.is_empty()
            && !name.starts_with('-')
            && !name.ends_with('-')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !is_kebab {
            return Err(malformed());
        }
        let instrument = Instrument::ALL
            .into_iter()
            .find(|i| i.sound_prefix() == prefix)
            .ok_or_else(|| SoundIdError::UnknownInstrument(id.to_string()))?;
        let sound = Sound::built_in(id, instrument);
        if instrument == Instrument::Drums && !sound.is_built_in() {
            return Err(SoundIdError::UnknownDrumKit(id.to_string()));
        }
        Ok(sound)
    }

    /// The id written in files, `"<instrument>/<sound>"`.
    pub fn id(&self) -> &str {
        // Only ever built from an ASCII id (`built_in`, `parse`), so this never fails.
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }

    /// The instrument family the sound belongs to, the one its id starts with.
    pub fn instrument(self) -> Instrument {
        self.instrument
    }

    /// The entry of the built-in table, or `None` for a sound a pack declares.
    pub fn built_in_entry(self) -> Option<&'static BuiltInSound> {
        BUILT_IN_SOUNDS.iter().find(|entry| entry.sound == self)
    }

    /// True for a sound of the built-in table, false for one a pack declares.
    pub fn is_built_in(self) -> bool {
        self.built_in_entry().is_some()
    }

    /// The name of a built-in sound, within its instrument. A pack's sound has the name its pack
    /// gives it, which this crate does not know.
    pub fn name(self) -> Option<&'static str> {
        self.built_in_entry().map(|entry| entry.name)
    }

    /// The General MIDI program of a built-in sound (the kit of a drum kit). A pack's sound plays
    /// the program its track recorded (`Track::midi_program`).
    pub fn midi_program(self) -> Option<u8> {
        self.built_in_entry().map(|entry| entry.midi_program)
    }

    /// True for a drum kit, which plays on the percussion channel.
    pub fn is_drum_kit(self) -> bool {
        self.instrument == Instrument::Drums
    }

    /// The drum kit for the program of a percussion channel, when a file names one. The kits of
    /// the General MIDI 2 and GS sets come in groups of eight from the standard one (the TR-808 is
    /// electronic); any other program, or none, gives the standard kit.
    pub fn from_drum_kit(program: Option<u8>) -> Sound {
        match program {
            Some(8..=15) => Sound::DRUMS_ROOM,
            Some(16..=23) => Sound::DRUMS_POWER,
            Some(24..=31) => Sound::DRUMS_ELECTRONIC,
            Some(32..=39) => Sound::DRUMS_JAZZ,
            Some(40..=47) => Sound::DRUMS_BRUSH,
            _ => Sound::DRUMS_STANDARD,
        }
    }

    /// The closest sound to a General MIDI program, when a file names one. Programs of the same
    /// family without a sound of their own go to the nearest one (jazz guitar plays clean,
    /// fretless bass plays finger, an accordion the reed organ, a sweep pad the warm pad). Every
    /// other program is an instrument that has no family here (chromatic percussion, winds,
    /// brass, other strings...): it gets the grand piano, whose track keeps every pitch. No
    /// program at all gives the nylon guitar, as files written for a tablature often name none.
    /// See `docs/sounds.md`.
    pub fn from_midi_program(program: Option<u8>) -> Sound {
        match program {
            None | Some(24) => Sound::GUITAR_NYLON,
            Some(25) => Sound::GUITAR_STEEL,
            Some(26 | 27 | 31) => Sound::ELECTRIC_GUITAR_CLEAN,
            Some(28) => Sound::ELECTRIC_GUITAR_MUTED,
            Some(29) => Sound::ELECTRIC_GUITAR_OVERDRIVE,
            Some(30) => Sound::ELECTRIC_GUITAR_DISTORTION,
            Some(34) => Sound::BASS_PICK,
            Some(36 | 37) => Sound::BASS_SLAP,
            Some(32..=39) => Sound::BASS_FINGER,
            Some(40) => Sound::VIOLIN,
            Some(105) => Sound::BANJO,
            // Electric grand and honky-tonk sound closest to the bright piano, the harpsichord too
            // (plucked and bright), the clavinet to the electric piano.
            Some(1..=3 | 6) => Sound::PIANO_BRIGHT,
            Some(4 | 7) => Sound::PIANO_ELECTRIC,
            Some(5) => Sound::PIANO_ELECTRIC_FM,
            Some(16 | 17) => Sound::ORGAN_DRAWBAR,
            Some(18) => Sound::ORGAN_ROCK,
            Some(19) => Sound::ORGAN_CHURCH,
            // Reed organ, accordion, harmonica, tango accordion: all free reeds.
            Some(20..=23) => Sound::ORGAN_REED,
            Some(52) => Sound::VOICE_AAHS,
            Some(53) => Sound::VOICE_OOHS,
            Some(54) => Sound::VOICE_SYNTH,
            Some(80 | 82 | 83) => Sound::SYNTH_SQUARE_LEAD,
            Some(81 | 84..=87) => Sound::SYNTH_SAW_LEAD,
            Some(91) => Sound::SYNTH_CHOIR_PAD,
            // The other pads and the effects (rain, soundtrack, atmosphere...) are pads too.
            Some(88..=103) => Sound::SYNTH_WARM_PAD,
            Some(_) => Sound::PIANO,
        }
    }
}

impl fmt::Debug for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sound({:?})", self.id())
    }
}

impl fmt::Display for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl std::str::FromStr for Sound {
    type Err = SoundIdError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Sound::parse(id)
    }
}

impl Serialize for Sound {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for Sound {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;
        Sound::parse(&id).map_err(serde::de::Error::custom)
    }
}

impl Instrument {
    /// The sound a track of this instrument gets when nothing else is said (older files, new
    /// tracks, a change of instrument).
    pub fn default_sound(self) -> Sound {
        match self {
            Instrument::Guitar => Sound::GUITAR_NYLON,
            Instrument::ElectricGuitar => Sound::ELECTRIC_GUITAR_CLEAN,
            Instrument::Bass => Sound::BASS_FINGER,
            Instrument::Violin => Sound::VIOLIN,
            Instrument::Banjo => Sound::BANJO,
            Instrument::Piano => Sound::PIANO,
            Instrument::Organ => Sound::ORGAN_DRAWBAR,
            Instrument::Synth => Sound::SYNTH_SQUARE_LEAD,
            Instrument::Voice => Sound::VOICE_AAHS,
            Instrument::Drums => Sound::DRUMS_STANDARD,
        }
    }

    /// The first part of the ids of the sounds of this instrument, before the slash, such as
    /// `"electric-guitar"`.
    pub fn sound_prefix(self) -> &'static str {
        match self {
            Instrument::Guitar => "guitar",
            Instrument::ElectricGuitar => "electric-guitar",
            Instrument::Bass => "bass",
            Instrument::Violin => "violin",
            Instrument::Banjo => "banjo",
            Instrument::Piano => "piano",
            Instrument::Organ => "organ",
            Instrument::Synth => "synth",
            Instrument::Voice => "voice",
            Instrument::Drums => "drums",
        }
    }

    /// The built-in sounds of this instrument, the default one first.
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
                Sound::from_drum_kit(sound.midi_program())
            } else {
                Sound::from_midi_program(sound.midi_program())
            };
            assert_eq!(back, sound, "{sound:?}");
        }
    }

    #[test]
    fn a_kit_without_a_sound_of_its_own_gets_the_closest_one() {
        assert_eq!(Sound::from_drum_kit(Some(25)), Sound::DRUMS_ELECTRONIC);
        assert_eq!(Sound::from_drum_kit(Some(1)), Sound::DRUMS_STANDARD);
        assert_eq!(Sound::from_drum_kit(Some(48)), Sound::DRUMS_STANDARD);
        assert_eq!(Sound::from_drum_kit(None), Sound::DRUMS_STANDARD);
        assert!(!Sound::from_midi_program(Some(0)).is_drum_kit());
    }

    #[test]
    fn a_program_without_a_sound_of_its_own_gets_the_closest_one() {
        assert_eq!(
            Sound::from_midi_program(Some(26)),
            Sound::ELECTRIC_GUITAR_CLEAN
        );
        assert_eq!(Sound::from_midi_program(Some(35)), Sound::BASS_FINGER);
        assert_eq!(Sound::from_midi_program(Some(37)), Sound::BASS_SLAP);
        assert_eq!(Sound::from_midi_program(None), Sound::GUITAR_NYLON);
    }

    #[test]
    fn a_program_of_no_family_here_gets_the_grand_piano() {
        for program in [0, 8, 15, 41, 56, 73, 104, 127] {
            assert_eq!(
                Sound::from_midi_program(Some(program)),
                Sound::PIANO,
                "{program}"
            );
        }
    }

    #[test]
    fn keyboard_and_voice_programs_get_the_closest_sound_of_their_family() {
        let cases = [
            (2, Sound::PIANO_BRIGHT),
            (7, Sound::PIANO_ELECTRIC),
            (17, Sound::ORGAN_DRAWBAR),
            (21, Sound::ORGAN_REED),
            (23, Sound::ORGAN_REED),
            (82, Sound::SYNTH_SQUARE_LEAD),
            (87, Sound::SYNTH_SAW_LEAD),
            (88, Sound::SYNTH_WARM_PAD),
            (95, Sound::SYNTH_WARM_PAD),
            (103, Sound::SYNTH_WARM_PAD),
        ];
        for (program, sound) in cases {
            assert_eq!(Sound::from_midi_program(Some(program)), sound, "{program}");
        }
        for program in 16..=23 {
            assert_eq!(
                Instrument::from_midi_program(Some(program)),
                Instrument::Organ
            );
        }
        for program in 80..=103 {
            assert_eq!(
                Instrument::from_midi_program(Some(program)),
                Instrument::Synth
            );
        }
        for program in 52..=54 {
            assert_eq!(
                Instrument::from_midi_program(Some(program)),
                Instrument::Voice
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
            let mut names: Vec<&str> = instrument.sounds().filter_map(Sound::name).collect();
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
        for entry in BUILT_IN_SOUNDS {
            let row = format!(
                "id: '{}', name: '{}', program: {}",
                entry.sound.id(),
                entry.name,
                entry.midi_program
            );
            assert!(ui.contains(&row), "instruments.ts lacks {row}");
        }
    }

    #[test]
    fn every_built_in_id_parses_back_to_its_sound() {
        for sound in Sound::ALL {
            assert_eq!(Sound::parse(sound.id()), Ok(sound));
            assert!(sound.is_built_in());
            let back: Sound = serde_json::from_str(&format!("\"{}\"", sound.id())).unwrap();
            assert_eq!(back, sound);
        }
    }

    #[test]
    fn a_well_formed_id_of_an_instrument_is_a_sound_a_pack_may_declare() {
        let twelve = Sound::parse("guitar/twelve-string").unwrap();
        assert_eq!(twelve.id(), "guitar/twelve-string");
        assert_eq!(twelve.instrument(), Instrument::Guitar);
        assert!(!twelve.is_built_in());
        assert_eq!((twelve.name(), twelve.midi_program()), (None, None));
        let json = serde_json::to_string(&twelve).unwrap();
        assert_eq!(json, r#""guitar/twelve-string""#);
        assert_eq!(serde_json::from_str::<Sound>(&json).unwrap(), twelve);
        assert_eq!(
            Sound::parse("electric-guitar/fuzz-2").unwrap().instrument(),
            Instrument::ElectricGuitar
        );
    }

    #[test]
    fn a_malformed_id_is_refused() {
        let long = format!("guitar/{}", "a".repeat(MAX_SOUND_ID_LEN));
        for id in [
            "",
            "guitar",
            "guitar/",
            "/nylon",
            "guitar/Nylon",
            "guitar/-nylon",
            "guitar/nylon-",
            "guitar/ny lon",
            "guitar/ny/lon",
            "guitar/nylon\u{e9}",
            long.as_str(),
        ] {
            assert!(Sound::parse(id).is_err(), "{id:?} was accepted");
        }
        assert!(matches!(
            Sound::parse("ukulele/soprano"),
            Err(SoundIdError::UnknownInstrument(_))
        ));
        assert!(serde_json::from_str::<Sound>(r#""Guitar/nylon""#).is_err());
    }

    #[test]
    fn a_drum_kit_can_only_be_a_built_in_one() {
        assert_eq!(Sound::parse("drums/jazz"), Ok(Sound::DRUMS_JAZZ));
        assert!(matches!(
            Sound::parse("drums/my-kit"),
            Err(SoundIdError::UnknownDrumKit(_))
        ));
    }
}
