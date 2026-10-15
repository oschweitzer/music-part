//! Owns the audio output on a dedicated thread. A `cpal` stream is not `Send` on every platform,
//! so it never leaves the thread that created it; the rest of the app talks to it by message.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use audio_engine::{AudioOutput, PackBanks, PackFailure, Player, Program, SharedState, SoundPack};
use score_model::Score;

enum Command {
    Load(Box<Score>),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetAudible(Vec<bool>),
}

pub struct AudioController {
    commands: Mutex<Option<Sender<Command>>>,
    state: Arc<OnceLock<Arc<SharedState>>>,
    failure: Arc<Mutex<Option<String>>>,
    /// The sample files of packs that could not be used, as they are found out.
    pack_failures: Arc<Mutex<Vec<PackFailure>>>,
}

impl AudioController {
    /// Starts the audio thread without waiting for it: loading the SoundFont takes a moment and
    /// the window should not wait for it. A failure is reported by the first command instead.
    /// `packs` are the sound packs found, lowest priority first: the samples of one are read here,
    /// on the audio thread, when a score loaded uses one of its sounds.
    pub fn start(soundfont: PathBuf, packs: Vec<SoundPack>) -> Self {
        let (tx, rx) = mpsc::channel::<Command>();
        let state = Arc::new(OnceLock::new());
        let failure = Arc::new(Mutex::new(None));
        let pack_failures = Arc::new(Mutex::new(Vec::new()));

        let thread_state = Arc::clone(&state);
        let thread_failure = Arc::clone(&failure);
        let thread_pack_failures = Arc::clone(&pack_failures);
        let spawned = thread::Builder::new().name("audio".into()).spawn(move || {
            let mut banks = PackBanks::new(packs);
            let (output, sample_rate) = match open_output(&soundfont) {
                Ok(opened) => opened,
                Err(message) => {
                    eprintln!("Audio unavailable: {message}");
                    *thread_failure.lock().expect("failure lock") = Some(message);
                    return;
                }
            };
            let _ = thread_state.set(output.state());

            for command in rx {
                match command {
                    Command::Load(score) => {
                        let reported = banks.failures().len();
                        output.change_banks(banks.update(&score));
                        output.load(Program::new(&score, sample_rate));
                        if banks.failures().len() > reported {
                            for failure in &banks.failures()[reported..] {
                                eprintln!("{}", failure.error);
                            }
                            *thread_pack_failures.lock().expect("pack failures lock") =
                                banks.failures().to_vec();
                        }
                    }
                    Command::Play => output.play(),
                    Command::Pause => output.pause(),
                    Command::Stop => output.stop(),
                    Command::Seek(ticks) => output.seek(ticks),
                    Command::SetAudible(audible) => output.set_audible(audible),
                }
            }
        });

        if let Err(e) = spawned {
            *failure.lock().expect("failure lock") =
                Some(format!("cannot start the audio thread: {e}"));
        }
        Self {
            commands: Mutex::new(Some(tx)),
            state,
            failure,
            pack_failures,
        }
    }

    /// A controller that only ever reports `reason`, for when the SoundFont cannot even be located.
    pub fn unavailable(reason: String) -> Self {
        Self {
            commands: Mutex::new(None),
            state: Arc::new(OnceLock::new()),
            failure: Arc::new(Mutex::new(Some(reason))),
            pack_failures: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn load(&self, score: Score) -> Result<(), String> {
        self.send(Command::Load(Box::new(score)))
    }

    pub fn play(&self) -> Result<(), String> {
        self.send(Command::Play)
    }

    pub fn pause(&self) -> Result<(), String> {
        self.send(Command::Pause)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.send(Command::Stop)
    }

    /// Moves to a position in 1/128 of a whole note.
    pub fn seek(&self, ticks: f64) -> Result<(), String> {
        self.send(Command::Seek(ticks))
    }

    /// Which tracks make sound, by track index.
    pub fn set_audible(&self, audible: Vec<bool>) -> Result<(), String> {
        self.send(Command::SetAudible(audible))
    }

    /// Playback state published by the audio callback, once the output is open.
    pub fn state(&self) -> Option<Arc<SharedState>> {
        self.state.get().cloned()
    }

    /// The sample files of packs that could not be used so far. A file is read when a score first
    /// uses its sound, so this grows as scores are opened.
    pub fn pack_failures(&self) -> Vec<PackFailure> {
        self.pack_failures
            .lock()
            .expect("pack failures lock")
            .clone()
    }

    fn send(&self, command: Command) -> Result<(), String> {
        if let Some(message) = self.failure.lock().expect("failure lock").clone() {
            return Err(message);
        }
        let commands = self.commands.lock().expect("commands lock");
        let sender = commands.as_ref().ok_or("audio is not available")?;
        // A closed channel means the audio thread has stopped: it recorded why just before.
        sender.send(command).map_err(|_| {
            self.failure
                .lock()
                .expect("failure lock")
                .clone()
                .unwrap_or_else(|| "the audio thread has stopped".into())
        })
    }
}

fn open_output(soundfont: &Path) -> Result<(AudioOutput, u32), String> {
    let sample_rate =
        AudioOutput::default_sample_rate().map_err(|e| format!("no audio output: {e}"))?;
    let mut file = File::open(soundfont)
        .map_err(|e| format!("cannot open the SoundFont {}: {e}", soundfont.display()))?;
    let player = Player::new(&mut file, sample_rate).map_err(|e| e.to_string())?;
    let output =
        AudioOutput::start(player).map_err(|e| format!("cannot open the audio output: {e}"))?;
    Ok((output, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn unavailable_controller_reports_its_reason() {
        let audio = AudioController::unavailable("no SoundFont".into());
        assert_eq!(audio.play(), Err("no SoundFont".into()));
        assert_eq!(
            audio.load(score_model::demo_score()),
            Err("no SoundFont".into())
        );
    }

    #[test]
    fn missing_soundfont_is_reported_by_commands() {
        let audio = AudioController::start(PathBuf::from("/nonexistent/font.sf2"), Vec::new());

        // The thread fails shortly after start: no output device, or no such file.
        let deadline = Instant::now() + Duration::from_secs(5);
        let error = loop {
            match audio.play() {
                Err(message) => break message,
                Ok(()) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(()) => panic!("expected an error before the deadline"),
            }
        };
        assert!(!error.is_empty());
        assert!(audio.state().is_none());
    }

    /// Opens the real output device with the real SoundFont, but plays nothing. Skipped when the
    /// machine has no audio output or the SoundFont has not been fetched.
    #[test]
    fn opens_the_real_output_when_available() {
        let soundfont =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/soundfonts/GeneralUser-GS.sf2");
        if !soundfont.exists() || AudioOutput::default_sample_rate().is_err() {
            eprintln!("no SoundFont or no audio output, test skipped");
            return;
        }

        let audio = AudioController::start(soundfont, Vec::new());
        let deadline = Instant::now() + Duration::from_secs(10);
        let state = loop {
            if let Some(state) = audio.state() {
                break state;
            }
            assert!(
                Instant::now() < deadline,
                "the audio output did not open in time"
            );
            thread::sleep(Duration::from_millis(20));
        };

        assert!(!state.is_playing());
        assert_eq!(state.position_ticks(), 0.0);
        assert_eq!(audio.load(score_model::demo_score()), Ok(()));
        assert_eq!(audio.stop(), Ok(()));
    }
}
