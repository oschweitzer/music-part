//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! `cpal` output (`AudioOutput`), sound packs (`pack`) and SFZ instruments (`sfz`).

pub mod output;
pub mod pack;
pub mod player;
pub mod sequencer;
pub mod sfz;

pub use output::{AudioOutput, SharedState};
pub use pack::{PackError, SoundPack};
pub use player::{Player, PlayerError, Program};
pub use sfz::{SfzError, SfzInstrument, SfzReport};
