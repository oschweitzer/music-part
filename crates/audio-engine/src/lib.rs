//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! `cpal` output (`AudioOutput`) and sound packs (`pack`).

pub mod output;
pub mod pack;
pub mod player;
pub mod sequencer;

pub use output::{AudioOutput, SharedState};
pub use pack::{PackError, SoundPack};
pub use player::{Player, PlayerError, Program};
