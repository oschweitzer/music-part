//! Audio engine: pure sequencer, SoundFont player (`Player`, no hardware)
//! `cpal` output (`AudioOutput`), sound packs (`pack`), their loading on demand (`banks`) and
//! SFZ instruments (`sfz`).

pub mod banks;
pub mod output;
pub mod pack;
pub mod player;
pub mod sequencer;
pub mod sfz;
#[cfg(test)]
mod test_fonts;

pub use banks::{BankChange, PackBanks, PackFailure};
pub use output::{AudioOutput, SharedState};
pub use pack::{declared_sounds, DeclaredSound, PackError, PackNote, SoundPack};
pub use player::{BankFont, Player, PlayerError, Program, RemovedBank};
pub use sfz::{SfzError, SfzInstrument, SfzReport};
