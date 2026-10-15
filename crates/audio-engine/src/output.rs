use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::banks::{self, BankChange};
use crate::player::{BankFont, Player, Program};

enum Command {
    Play,
    Pause,
    Stop,
    Load(Box<Program>),
    Seek(f64),
    SetAudible(Vec<bool>),
    ChangeBank(Box<BankChange>),
}

/// Playback state published by the audio callback for other threads to read, without locks.
#[derive(Default)]
pub struct SharedState {
    position_ticks: AtomicU64,
    is_playing: AtomicBool,
}

impl SharedState {
    /// Playback position in 1/128 of a whole note.
    pub fn position_ticks(&self) -> f64 {
        f64::from_bits(self.position_ticks.load(Ordering::Relaxed))
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    fn publish(&self, player: &Player) {
        self.position_ticks
            .store(player.position_ticks().to_bits(), Ordering::Relaxed);
        self.is_playing
            .store(player.is_playing(), Ordering::Relaxed);
    }
}

/// `cpal` output stream. The `Player` lives in the audio callback; the rest of the app talks to
/// it through commands and reads its state through [`SharedState`].
///
/// Loading a score swaps a prebuilt [`Program`] in the callback. The old one is freed there,
/// which is the one allocation-related exception to the real-time rule: it only happens on a
/// user action, while playback is stopped. Sample banks are read before they reach the callback,
/// and the samples of a removed bank are sent back to be freed by the next call to
/// [`AudioOutput::change_banks`], not in the callback.
pub struct AudioOutput {
    _stream: cpal::Stream,
    commands: Sender<Command>,
    state: Arc<SharedState>,
    removed: Receiver<BankFont>,
}

impl AudioOutput {
    /// Sample rate of the default output device, to create the `Player` with.
    pub fn default_sample_rate() -> Result<u32, Box<dyn std::error::Error>> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        Ok(device.default_output_config()?.sample_rate())
    }

    /// Opens the default output device at the player's sample rate.
    pub fn start(mut player: Player) -> Result<Self, Box<dyn std::error::Error>> {
        let rate = player.sample_rate();
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let supported = device
            .supported_output_configs()?
            .filter(|r| r.sample_format() == cpal::SampleFormat::F32 && r.channels() >= 2)
            .find_map(|r| r.try_with_sample_rate(rate))
            .ok_or_else(|| format!("the device does not support f32 stereo output at {rate} Hz"))?;
        let config = supported.config();
        let channels = usize::from(config.channels);

        let (commands, rx): (Sender<Command>, Receiver<Command>) = mpsc::channel();
        let state = Arc::new(SharedState::default());
        let published = Arc::clone(&state);
        let (removed_tx, removed) = mpsc::channel::<BankFont>();
        let mut stereo: Vec<f32> = Vec::new();

        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _| {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Command::Play => player.play(),
                        Command::Pause => player.pause(),
                        Command::Stop => player.stop(),
                        Command::Load(program) => player.load(*program),
                        Command::Seek(ticks) => player.seek(ticks),
                        Command::SetAudible(audible) => player.set_audible(audible),
                        Command::ChangeBank(change) => {
                            if let Some(font) = banks::apply(&mut player, *change) {
                                let _ = removed_tx.send(font);
                            }
                        }
                    }
                }
                let frames = data.len() / channels;
                stereo.resize(frames * 2, 0.0);
                player.render(&mut stereo);
                published.publish(&player);

                let (lr_frames, _) = stereo.as_chunks::<2>();
                for (frame, lr) in data.chunks_exact_mut(channels).zip(lr_frames) {
                    frame.fill(0.0);
                    frame[0] = lr[0];
                    frame[1] = lr[1];
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )?;
        stream.play()?;

        Ok(Self {
            _stream: stream,
            commands,
            state,
            removed,
        })
    }

    pub fn state(&self) -> Arc<SharedState> {
        Arc::clone(&self.state)
    }

    /// Adds and removes sample banks; send them before the score that needs them. Frees the
    /// samples of the banks removed by earlier calls.
    pub fn change_banks(&self, changes: Vec<BankChange>) {
        while self.removed.try_recv().is_ok() {}
        for change in changes {
            let _ = self.commands.send(Command::ChangeBank(Box::new(change)));
        }
    }

    pub fn load(&self, program: Program) {
        let _ = self.commands.send(Command::Load(Box::new(program)));
    }

    pub fn play(&self) {
        let _ = self.commands.send(Command::Play);
    }

    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// Moves to a position in 1/128 of a whole note.
    pub fn seek(&self, ticks: f64) {
        let _ = self.commands.send(Command::Seek(ticks));
    }

    /// Which tracks make sound, by track index. Kept when a new score is loaded.
    pub fn set_audible(&self, audible: Vec<bool>) {
        let _ = self.commands.send(Command::SetAudible(audible));
    }
}
