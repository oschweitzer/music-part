use std::io::{Read, Seek};

use oxisynth::{MidiEvent, SoundFont, Synth, SynthDescriptor};
use score_model::{Instrument, Score};

use crate::sequencer::{self, Event};

/// Silence laissé après le dernier événement pour la fin des résonances.
const RELEASE_TAIL_SECS: u64 = 2;

#[derive(Debug)]
pub enum PlayerError {
    SoundFont(String),
    Synth(String),
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SoundFont(e) => write!(f, "SoundFont invalide : {e}"),
            Self::Synth(e) => write!(f, "erreur du synthétiseur : {e}"),
        }
    }
}

impl std::error::Error for PlayerError {}

/// Lit une partition via un synthé SoundFont. Ne touche pas au matériel audio :
/// `render` remplit un buffer stéréo entrelacé, donc utilisable hors-ligne.
pub struct Player {
    synth: Synth,
    events: Vec<Event>,
    /// Position de départ en échantillons de chaque événement (même ordre que `events`).
    event_samples: Vec<u64>,
    end_sample: u64,
    next: usize,
    position: u64,
    playing: bool,
    channels: Vec<u8>,
}

impl Player {
    pub fn new<R: Read + Seek>(
        score: &Score,
        soundfont: &mut R,
        sample_rate: u32,
    ) -> Result<Self, PlayerError> {
        let mut synth = Synth::new(SynthDescriptor {
            sample_rate: sample_rate as f32,
            ..Default::default()
        })
        .map_err(|e| PlayerError::Synth(format!("{e:?}")))?;
        // Le parseur SF2 panique sur certains fichiers corrompus : on le transforme en erreur.
        let font =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| SoundFont::load(soundfont)))
                .map_err(|_| PlayerError::SoundFont("fichier corrompu ou non SF2".into()))?
                .map_err(|e| PlayerError::SoundFont(format!("{e:?}")))?;
        synth.add_font(font, true);

        // Un canal MIDI par piste, en évitant le canal 9 (percussions).
        let channels: Vec<u8> = (0..score.tracks.len())
            .map(|i| {
                let c = (i % 15) as u8;
                if c >= 9 {
                    c + 1
                } else {
                    c
                }
            })
            .collect();
        for (track, &channel) in score.tracks.iter().zip(&channels) {
            let program_id = match track.instrument {
                Instrument::Guitar => 24, // Nylon Guitar
                Instrument::Bass => 33,   // Electric Bass (finger)
                Instrument::Violin => 40,
            };
            synth
                .send_event(MidiEvent::ProgramChange {
                    channel,
                    program_id,
                })
                .map_err(|e| PlayerError::Synth(format!("{e:?}")))?;
        }

        let events = sequencer::schedule(score);
        let event_samples: Vec<u64> = events
            .iter()
            .map(|e| sequencer::tick_to_sample(e.tick, score.tempo_bpm, sample_rate))
            .collect();
        let end_sample =
            event_samples.last().copied().unwrap_or(0) + RELEASE_TAIL_SECS * u64::from(sample_rate);

        Ok(Self {
            synth,
            events,
            event_samples,
            end_sample,
            next: 0,
            position: 0,
            playing: false,
            channels,
        })
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
        self.silence();
    }

    /// Arrête et revient au début.
    pub fn stop(&mut self) {
        self.pause();
        self.next = 0;
        self.position = 0;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Position de lecture en échantillons.
    pub fn position(&self) -> u64 {
        self.position
    }

    fn silence(&mut self) {
        for &channel in &self.channels {
            let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
        }
    }

    /// Remplit `out` (stéréo entrelacé L,R,L,R...). Écrit du silence si en pause ou terminé.
    pub fn render(&mut self, out: &mut [f32]) {
        let (frames, _) = out.as_chunks_mut::<2>();
        for frame in frames {
            if !self.playing {
                frame.fill(0.0);
                continue;
            }
            while self.next < self.events.len() && self.event_samples[self.next] <= self.position {
                self.dispatch(self.events[self.next]);
                self.next += 1;
            }
            let (l, r) = self.synth.read_next();
            frame[0] = l;
            frame[1] = r;
            self.position += 1;
            if self.position >= self.end_sample {
                self.playing = false;
                self.silence();
            }
        }
    }

    fn dispatch(&mut self, e: Event) {
        let channel = self.channels[e.track];
        let event = if e.on {
            MidiEvent::NoteOn {
                channel,
                key: e.pitch,
                vel: e.velocity,
            }
        } else {
            MidiEvent::NoteOff {
                channel,
                key: e.pitch,
            }
        };
        // Une note hors plage ne doit pas interrompre la lecture.
        let _ = self.synth.send_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::*;
    use std::fs::File;

    fn one_note_score() -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            tracks: vec![Track {
                name: "g".into(),
                instrument: Instrument::Guitar,
                tuning: GUITAR_STANDARD.to_vec(),
                measures: vec![Measure {
                    time_signature: TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    },
                    voices: vec![Voice {
                        beats: vec![Beat {
                            duration: Duration {
                                value: NoteValue::Whole,
                                dots: 0,
                            },
                            notes: vec![Note {
                                string: 1,
                                fret: 0,
                                technique: None,
                            }],
                        }],
                    }],
                }],
            }],
        }
    }

    #[test]
    fn invalid_soundfont_is_an_error() {
        let mut junk = std::io::Cursor::new(vec![0u8; 64]);
        assert!(matches!(
            Player::new(&one_note_score(), &mut junk, 44_100),
            Err(PlayerError::SoundFont(_))
        ));
    }

    /// Nécessite un SoundFont : `SOUNDFONT=/usr/share/sounds/sf2/TimGM6mb.sf2 cargo test`.
    /// Sans la variable, le test est ignoré.
    #[test]
    fn renders_sound_only_while_playing() {
        let Ok(path) = std::env::var("SOUNDFONT") else {
            eprintln!("SOUNDFONT non défini, test ignoré");
            return;
        };
        let mut player =
            Player::new(&one_note_score(), &mut File::open(path).unwrap(), 44_100).unwrap();
        let mut buf = vec![0.0f32; 2 * 4410];

        player.render(&mut buf);
        assert!(
            buf.iter().all(|s| *s == 0.0),
            "silence attendu avant play()"
        );

        player.play();
        player.render(&mut buf);
        player.render(&mut buf);
        assert!(
            buf.iter().any(|s| s.abs() > 1e-4),
            "du son attendu après play()"
        );

        player.pause();
        player.render(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.0));
        assert!(player.position() > 0);
        player.stop();
        assert_eq!(player.position(), 0);
    }
}
