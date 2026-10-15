//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! and `cpal` output (`AudioOutput`).

pub mod output;
pub mod player;
pub mod sequencer;

pub use output::{AudioOutput, SharedState};
pub use player::{Player, PlayerError, Program};

/// A SoundFont of recordings of one sound of an instrument, fetched by `pnpm fetch:soundfont` into
/// `assets/soundfonts/banks/` and played instead of the General MIDI program of that sound. The
/// other sounds of the instrument keep their General MIDI program.
pub struct SampleBank {
    pub sound: score_model::Sound,
    pub file: &'static str,
    /// First and last MIDI key the recordings cover. Notes outside are played with the General
    /// MIDI sound. A test checks these against the files.
    pub first_key: u8,
    pub last_key: u8,
}

impl SampleBank {
    pub fn keys(&self) -> std::ops::RangeInclusive<u8> {
        self.first_key..=self.last_key
    }
}

pub const SAMPLE_BANKS: [SampleBank; 3] = [
    SampleBank {
        sound: score_model::Sound::GuitarNylon,
        file: "nylon-guitar.sf2",
        first_key: 29,
        last_key: 88,
    },
    SampleBank {
        sound: score_model::Sound::ElectricGuitarClean,
        file: "electric-guitar.sf2",
        first_key: 35,
        last_key: 86,
    },
    SampleBank {
        sound: score_model::Sound::BassFinger,
        file: "electric-bass.sf2",
        first_key: 26,
        last_key: 45,
    },
];
