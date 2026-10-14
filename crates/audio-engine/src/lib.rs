//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! and `cpal` output (`AudioOutput`).

pub mod output;
pub mod player;
pub mod sequencer;

pub use output::{AudioOutput, SharedState};
pub use player::{Player, PlayerError, Program};

/// A SoundFont of recordings of one instrument, fetched by `pnpm fetch:soundfont` into
/// `assets/soundfonts/banks/` and played instead of the General MIDI sound of that instrument.
pub struct SampleBank {
    pub instrument: score_model::Instrument,
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
        instrument: score_model::Instrument::Guitar,
        file: "nylon-guitar.sf2",
        first_key: 29,
        last_key: 88,
    },
    SampleBank {
        instrument: score_model::Instrument::ElectricGuitar,
        file: "electric-guitar.sf2",
        first_key: 35,
        last_key: 86,
    },
    SampleBank {
        instrument: score_model::Instrument::Bass,
        file: "electric-bass.sf2",
        first_key: 26,
        last_key: 45,
    },
];
