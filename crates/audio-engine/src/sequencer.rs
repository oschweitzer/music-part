use std::collections::HashMap;

use score_model::Score;

/// Vélocité MIDI utilisée tant que le modèle n'a pas de dynamiques.
const DEFAULT_VELOCITY: u8 = 90;
/// Une noire = 32 unités (le modèle compte en 1/128 de ronde).
const TICKS_PER_QUARTER: f64 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// Position en 1/128 de ronde depuis le début du morceau.
    pub tick: u32,
    pub track: usize,
    pub pitch: u8,
    pub velocity: u8,
    pub on: bool,
}

/// Aplatit la partition en événements triés. À tick égal, les extinctions
/// passent avant les attaques pour qu'une même hauteur répétée soit rejouée.
/// Une note liée (`tied`) n'émet pas d'attaque : elle repousse l'extinction de
/// la même hauteur jouée au temps précédent de la voix.
pub fn schedule(score: &Score) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();

    for (track_idx, track) in score.tracks.iter().enumerate() {
        let voice_count = track
            .measures
            .iter()
            .map(|m| m.voices.len())
            .max()
            .unwrap_or(0);

        for voice_idx in 0..voice_count {
            // Hauteur -> index de son événement d'extinction, pour les notes encore tenues.
            let mut sounding: HashMap<u8, usize> = HashMap::new();
            let mut measure_start = 0;

            for measure in &track.measures {
                let mut tick = measure_start;
                for beat in measure
                    .voices
                    .get(voice_idx)
                    .map_or(&[][..], |v| &v.beats[..])
                {
                    let end = tick + beat.duration.ticks();
                    let mut next_sounding = HashMap::new();

                    for note in &beat.notes {
                        let Some(pitch) = track.pitch(note) else {
                            continue;
                        };
                        let held = note.tied.then(|| sounding.get(&pitch)).flatten();
                        let off_idx = if let Some(&idx) = held {
                            events[idx].tick = end;
                            idx
                        } else {
                            events.push(Event {
                                tick,
                                track: track_idx,
                                pitch,
                                velocity: DEFAULT_VELOCITY,
                                on: true,
                            });
                            events.push(Event {
                                tick: end,
                                track: track_idx,
                                pitch,
                                velocity: 0,
                                on: false,
                            });
                            events.len() - 1
                        };
                        next_sounding.insert(pitch, off_idx);
                    }

                    sounding = next_sounding;
                    tick = end;
                }
                measure_start += measure.time_signature.ticks();
            }
        }
    }

    events.sort_by_key(|e| (e.tick, e.on));
    events
}

/// Convertit une position en échantillons pour un tempo et une fréquence donnés.
pub fn tick_to_sample(tick: u32, tempo_bpm: u16, sample_rate: u32) -> u64 {
    let seconds_per_tick = 60.0 / (f64::from(tempo_bpm.max(1)) * TICKS_PER_QUARTER);
    (f64::from(tick) * seconds_per_tick * f64::from(sample_rate)).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use score_model::*;

    fn quarter_note(string: u8, fret: u8) -> Beat {
        Beat {
            duration: Duration {
                value: NoteValue::Quarter,
                dots: 0,
            },
            notes: vec![Note {
                string,
                fret,
                technique: None,
                tied: false,
            }],
        }
    }

    fn score_with(beats: Vec<Beat>) -> Score {
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
                    voices: vec![Voice { beats }],
                }],
            }],
        }
    }

    #[test]
    fn repeated_pitch_is_retriggered() {
        let events = schedule(&score_with(vec![quarter_note(1, 0), quarter_note(1, 0)]));
        // on@0, off@32, on@32, off@64 : l'extinction précède la ré-attaque.
        let seq: Vec<(u32, bool)> = events.iter().map(|e| (e.tick, e.on)).collect();
        assert_eq!(seq, vec![(0, true), (32, false), (32, true), (64, false)]);
    }

    #[test]
    fn rests_produce_no_events() {
        let rest = Beat {
            duration: Duration {
                value: NoteValue::Quarter,
                dots: 0,
            },
            notes: vec![],
        };
        let events = schedule(&score_with(vec![rest, quarter_note(2, 1)]));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].tick, 32);
        assert_eq!(events[0].pitch, 60); // corde 2 = Si3 (59) + frette 1
    }

    #[test]
    fn tied_note_extends_the_previous_one_across_measures() {
        let tied = |tied| Beat {
            duration: Duration {
                value: NoteValue::Whole,
                dots: 0,
            },
            notes: vec![Note {
                string: 1,
                fret: 0,
                technique: None,
                tied,
            }],
        };
        let measure = |beat| Measure {
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            voices: vec![Voice { beats: vec![beat] }],
        };
        let mut score = score_with(vec![]);
        score.tracks[0].measures = vec![measure(tied(false)), measure(tied(true))];

        let seq: Vec<(u32, bool)> = schedule(&score).iter().map(|e| (e.tick, e.on)).collect();
        assert_eq!(seq, vec![(0, true), (256, false)]);
    }

    #[test]
    fn tie_to_a_different_pitch_is_replayed() {
        let beat = |fret, tied| Beat {
            duration: Duration {
                value: NoteValue::Quarter,
                dots: 0,
            },
            notes: vec![Note {
                string: 1,
                fret,
                technique: None,
                tied,
            }],
        };
        let events = schedule(&score_with(vec![beat(0, false), beat(2, true)]));
        assert_eq!(events.iter().filter(|e| e.on).count(), 2);
    }

    #[test]
    fn tempo_conversion() {
        // 120 bpm : une noire = 0,5 s = 22050 échantillons à 44,1 kHz.
        assert_eq!(tick_to_sample(32, 120, 44_100), 22_050);
    }

    #[test]
    fn midi_import_feeds_the_sequencer() {
        use midly::{
            num::u4, Format, Header, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
        };
        let on = |d: u32, on: bool| TrackEvent {
            delta: d.into(),
            kind: TrackEventKind::Midi {
                channel: u4::new(0),
                message: if on {
                    MidiMessage::NoteOn {
                        key: 64.into(),
                        vel: 100.into(),
                    }
                } else {
                    MidiMessage::NoteOff {
                        key: 64.into(),
                        vel: 0.into(),
                    }
                },
            },
        };
        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(480.into())),
            tracks: vec![vec![on(0, true), on(480, false)]],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();
        let score = io_midi::import_midi(&bytes).unwrap();
        let events = schedule(&score);
        assert_eq!(
            (events[0].tick, events[0].pitch, events[0].on),
            (0, 64, true)
        );
        assert_eq!((events[1].tick, events[1].on), (32, false));
    }
}
