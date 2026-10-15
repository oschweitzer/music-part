//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! `cpal` output (`AudioOutput`), sound packs (`pack`) and their loading on demand (`banks`).

pub mod banks;
pub mod output;
pub mod pack;
pub mod player;
pub mod sequencer;
#[cfg(test)]
mod test_fonts;

pub use banks::{BankChange, PackBanks, PackFailure};
pub use output::{AudioOutput, SharedState};
pub use pack::{PackError, SoundPack};
pub use player::{BankFont, Player, PlayerError, Program};
