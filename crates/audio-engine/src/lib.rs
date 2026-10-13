//! Moteur audio : séquenceur pur, lecteur SoundFont (`Player`, sans matériel)
//! et sortie `cpal` (`AudioOutput`).

pub mod output;
pub mod player;
pub mod sequencer;

pub use output::AudioOutput;
pub use player::{Player, PlayerError};
