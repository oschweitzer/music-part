use std::sync::mpsc::{self, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::player::Player;

#[derive(Debug, Clone, Copy)]
enum Command {
    Play,
    Pause,
    Stop,
}

/// Flux de sortie `cpal`. Le `Player` vit dans le thread audio ; l'UI ne
/// communique que par commandes, sans verrou dans le callback temps réel.
pub struct AudioOutput {
    _stream: cpal::Stream,
    commands: Sender<Command>,
    sample_rate: u32,
}

impl AudioOutput {
    /// Fréquence du périphérique de sortie par défaut, à passer à `Player::new`.
    pub fn default_sample_rate() -> Result<u32, Box<dyn std::error::Error>> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("aucun périphérique de sortie audio")?;
        Ok(device.default_output_config()?.sample_rate())
    }

    pub fn start(mut player: Player) -> Result<Self, Box<dyn std::error::Error>> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("aucun périphérique de sortie audio")?;
        let supported = device
            .supported_output_configs()?
            .find_map(|r| {
                (r.sample_format() == cpal::SampleFormat::F32 && r.channels() >= 2)
                    .then(|| r.with_max_sample_rate())
            })
            .ok_or("le périphérique ne supporte pas la sortie f32 stéréo")?;
        let config = supported.config();
        let channels = usize::from(config.channels);
        let sample_rate = config.sample_rate;

        let (commands, rx): (Sender<Command>, Receiver<Command>) = mpsc::channel();
        let mut stereo: Vec<f32> = Vec::new();

        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _| {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Command::Play => player.play(),
                        Command::Pause => player.pause(),
                        Command::Stop => player.stop(),
                    }
                }
                let frames = data.len() / channels;
                stereo.resize(frames * 2, 0.0);
                player.render(&mut stereo);
                let (lr_frames, _) = stereo.as_chunks::<2>();
                for (frame, lr) in data.chunks_exact_mut(channels).zip(lr_frames) {
                    frame.fill(0.0);
                    frame[0] = lr[0];
                    frame[1] = lr[1];
                }
            },
            |err| eprintln!("erreur du flux audio : {err}"),
            None,
        )?;
        stream.play()?;

        Ok(Self {
            _stream: stream,
            commands,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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
}
