//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! and `cpal` output (`AudioOutput`).

pub mod output;
pub mod player;
pub mod sequencer;

pub use output::AudioOutput;
pub use player::{Player, PlayerError};
