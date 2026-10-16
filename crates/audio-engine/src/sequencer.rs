use std::collections::HashMap;

use score_model::{Beat, Note, NoteKind, Score, Technique, Track};

/// MIDI velocity used until the model has dynamics.
const DEFAULT_VELOCITY: u8 = 90;
/// Hammer-ons, pull-offs and the note a slide lands on are not picked: they are softer.
const LEGATO_VELOCITY: u8 = 62;
const HARMONIC_VELOCITY: u8 = 70;
const PALM_MUTE_VELOCITY: u8 = 76;
/// A quarter note is 32 units (the model counts in 1/128 of a whole note).
const TICKS_PER_QUARTER: f64 = 32.0;

/// Pitch bend centre, and the number of semitones the full wheel travel covers. The player sets
/// the same range on every channel.
pub const BEND_CENTER: u16 = 8192;
pub const BEND_RANGE_SEMITONES: i32 = 12;
/// A bend reaches this many semitones above the note (a whole tone).
const BEND_SEMITONES: i32 = 2;
/// Granularity of bend and slide ramps: a 32nd note.
const RAMP_STEP: u32 = 4;
/// A slide only travels this far; a wider jump is played as two plain notes.
const MAX_SLIDE_SEMITONES: i32 = BEND_RANGE_SEMITONES;

/// The tone colour a note is played with. The player gives each one its own MIDI channel, set up
/// differently, because a MIDI note has no way to say "muted".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timbre {
    Open,
    /// Palm mute: dull and short.
    Muted,
    /// Natural harmonic: pure, without the pick.
    Harmonic,
    /// Hammer-on, pull-off and the landing of a slide: no pick attack.
    Legato,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// Position in 1/128 of a whole note since the start of the piece.
    pub tick: u32,
    pub track: usize,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NoteOn {
        pitch: u8,
        velocity: u8,
        timbre: Timbre,
    },
    NoteOff {
        pitch: u8,
        timbre: Timbre,
    },
    /// 14-bit pitch wheel value, `BEND_CENTER` being no bend. It moves the whole channel, that is
    /// the whole track: a bent note bends the other notes sounding with it.
    PitchBend(u16),
}

impl Action {
    /// At equal ticks: note-offs first so a repeated pitch is retriggered, then the wheel back to
    /// centre, then new notes.
    fn order(self) -> u8 {
        match self {
            Action::NoteOff { .. } => 0,
            Action::PitchBend(_) => 1,
            Action::NoteOn { .. } => 2,
        }
    }
}

/// Where the events of a note still sounding can be pushed back by a following tie.
#[derive(Clone, Copy)]
struct Sounding {
    off: usize,
    /// The event that puts the wheel back to centre, when the note bent it.
    reset: Option<usize>,
}

/// Flattens the score into sorted events.
///
/// A tied note (`tied`) emits no note-on: it pushes back the note-off of the same pitch played on
/// the previous beat of the voice. Techniques shape the sound:
/// - hammer-on and pull-off are played softly, without the pick attack (the legato timbre);
/// - a slide glides, in the second half of the note, to the note of the same string that follows,
///   which is then played softly, with the legato timbre;
/// - a bend rises a whole tone at the start of the note and is held;
/// - a palm mute is quieter and cut short, and played with the muted timbre;
/// - a natural harmonic sounds at the pitch of the harmonic, quieter, with the harmonic timbre.
///
/// A drum track is scheduled apart (see `schedule_drums`): its notes are the keys of its pieces,
/// and a hit rings on instead of stopping at the end of its beat.
pub fn schedule(score: &Score) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();

    for (track_idx, track) in score.tracks.iter().enumerate() {
        if track.instrument.note_kind() == NoteKind::Percussion {
            schedule_drums(track_idx, track, &mut events);
            continue;
        }
        let voice_count = track
            .measures
            .iter()
            .map(|m| m.voices.len())
            .max()
            .unwrap_or(0);

        for voice_idx in 0..voice_count {
            // Every beat of the voice with its start, across barlines.
            let mut beats: Vec<(u32, &Beat)> = Vec::new();
            let mut measure_start = 0;
            for measure in &track.measures {
                let mut tick = measure_start;
                for beat in measure
                    .voices
                    .get(voice_idx)
                    .map_or(&[][..], |v| &v.beats[..])
                {
                    beats.push((tick, beat));
                    tick += beat.duration.ticks();
                }
                measure_start += measure.time_signature.ticks();
            }

            // Pitch -> the events of its note, for notes still being held.
            let mut sounding: HashMap<u8, Sounding> = HashMap::new();
            // Strings of the current beat that a slide of the previous beat landed on.
            let mut slid_onto: Vec<u8> = Vec::new();

            for (i, &(start, beat)) in beats.iter().enumerate() {
                let end = start + beat.duration.ticks();
                let mut next_sounding = HashMap::new();
                let mut next_slid_onto = Vec::new();

                for note in &beat.notes {
                    let Some(pitch) = sounding_pitch(track, note) else {
                        continue;
                    };
                    if let Some(held) = note.tied.then(|| sounding.get(&pitch)).flatten() {
                        events[held.off].tick = end;
                        if let Some(reset) = held.reset {
                            events[reset].tick = end;
                        }
                        next_sounding.insert(pitch, *held);
                        continue;
                    }

                    let is_soft = note.string().is_some_and(|s| slid_onto.contains(&s));
                    let velocity = velocity_of(note, is_soft);
                    let length = match note.technique {
                        Some(Technique::PalmMute) => beat.duration.ticks() / 2,
                        _ => beat.duration.ticks(),
                    };
                    let note_end = start + length;
                    let timbre = timbre_of(note, is_soft);

                    events.push(Event {
                        tick: start,
                        track: track_idx,
                        action: Action::NoteOn {
                            pitch,
                            velocity,
                            timbre,
                        },
                    });
                    let off = events.len();
                    events.push(Event {
                        tick: note_end,
                        track: track_idx,
                        action: Action::NoteOff { pitch, timbre },
                    });

                    let mut bends = Vec::new();
                    match note.technique {
                        Some(Technique::Bend) => {
                            let rise = (beat.duration.ticks() / 3).min(4 * RAMP_STEP);
                            bends = ramp(start, rise, BEND_SEMITONES);
                        }
                        Some(Technique::Slide) => {
                            if let Some(delta) = slide_target(track, note, beats.get(i + 1)) {
                                let half = beat.duration.ticks() / 2;
                                bends = ramp(start + half, half, delta);
                                next_slid_onto.extend(note.string());
                            }
                        }
                        _ => {}
                    }
                    let reset = if bends.is_empty() {
                        None
                    } else {
                        for tick_and_value in bends {
                            events.push(Event {
                                tick: tick_and_value.0,
                                track: track_idx,
                                action: Action::PitchBend(tick_and_value.1),
                            });
                        }
                        events.push(Event {
                            tick: note_end,
                            track: track_idx,
                            action: Action::PitchBend(BEND_CENTER),
                        });
                        Some(events.len() - 1)
                    };
                    next_sounding.insert(pitch, Sounding { off, reset });
                }

                sounding = next_sounding;
                slid_onto = next_slid_onto;
            }
        }
    }

    events.sort_by_key(|e| (e.tick, e.action.order()));
    events
}

/// The hits of a drum track, every voice. A drum is struck, not held: a hit rings until the same
/// piece is struck again or the track ends, so a crash written as an eighth note is not choked by
/// a note-off after an eighth (the kit's own release and exclusive classes, such as the closed
/// hi-hat cutting the open one, shape it). A tied hit is not struck again. Every hit has the
/// default velocity and the open timbre: techniques are for strings.
fn schedule_drums(track_index: usize, track: &Track, events: &mut Vec<Event>) {
    // (start, piece, tied) of every hit.
    let mut hits: Vec<(u32, u8, bool)> = Vec::new();
    let mut measure_start = 0;
    for measure in &track.measures {
        for voice in &measure.voices {
            let mut tick = measure_start;
            for beat in &voice.beats {
                for note in &beat.notes {
                    if let Some(piece) = track.pitch(note) {
                        hits.push((tick, piece, note.tied));
                    }
                }
                tick += beat.duration.ticks();
            }
        }
        measure_start += measure.time_signature.ticks();
    }
    hits.sort_unstable();
    // The same piece struck in two voices at once is one hit.
    hits.dedup_by_key(|(tick, piece, _)| (*tick, *piece));

    let event = |tick: u32, action: Action| Event {
        tick,
        track: track_index,
        action,
    };
    let off = |pitch: u8| Action::NoteOff {
        pitch,
        timbre: Timbre::Open,
    };
    let mut ringing: Vec<u8> = Vec::new();
    for (tick, pitch, tied) in hits {
        let is_ringing = ringing.contains(&pitch);
        if tied && is_ringing {
            continue;
        }
        if is_ringing {
            events.push(event(tick, off(pitch)));
        } else {
            ringing.push(pitch);
        }
        events.push(event(
            tick,
            Action::NoteOn {
                pitch,
                velocity: DEFAULT_VELOCITY,
                timbre: Timbre::Open,
            },
        ));
    }
    for pitch in ringing {
        events.push(event(measure_start, off(pitch)));
    }
}

/// The pitch a note sounds at, or `None` if it has no playable pitch.
fn sounding_pitch(track: &Track, note: &Note) -> Option<u8> {
    let pitch = track.pitch(note)?;
    // A natural harmonic sounds at fixed intervals above the open string; a note without a
    // string has none.
    let (Some(Technique::Harmonic), Some(fret)) = (note.technique, note.fret()) else {
        return Some(pitch);
    };
    let open = pitch - fret;
    let above_open = match fret {
        4 => 28,
        5 => 24,
        7 | 19 => 19,
        9 => 16,
        12 | 24 => 12,
        fret => u32::from(fret) + 12,
    };
    u8::try_from(u32::from(open) + above_open)
        .ok()
        .filter(|p| *p <= 127)
}

fn timbre_of(note: &Note, is_soft: bool) -> Timbre {
    match note.technique {
        Some(Technique::PalmMute) => Timbre::Muted,
        Some(Technique::Harmonic) => Timbre::Harmonic,
        Some(Technique::HammerOn | Technique::PullOff) => Timbre::Legato,
        _ if is_soft => Timbre::Legato,
        _ => Timbre::Open,
    }
}

fn velocity_of(note: &Note, is_soft: bool) -> u8 {
    match note.technique {
        Some(Technique::HammerOn | Technique::PullOff) => LEGATO_VELOCITY,
        Some(Technique::Harmonic) => HARMONIC_VELOCITY,
        Some(Technique::PalmMute) => PALM_MUTE_VELOCITY,
        _ if is_soft => LEGATO_VELOCITY,
        _ => DEFAULT_VELOCITY,
    }
}

/// Semitones from this note to the same string of the next beat, if a slide can reach it.
fn slide_target(track: &Track, note: &Note, next: Option<&(u32, &Beat)>) -> Option<i32> {
    let (_, beat) = next?;
    let string = note.string()?;
    let target = beat.notes.iter().find(|n| n.string() == Some(string))?;
    let delta = i32::from(track.pitch(target)?) - i32::from(track.pitch(note)?);
    (delta != 0 && delta.abs() <= MAX_SLIDE_SEMITONES).then_some(delta)
}

/// Pitch wheel steps rising linearly to `semitones` over `length` ticks from `start`, as
/// (tick, wheel value). The last step lands exactly on the target.
fn ramp(start: u32, length: u32, semitones: i32) -> Vec<(u32, u16)> {
    let steps = (length / RAMP_STEP).max(1);
    (1..=steps)
        .map(|k| {
            let progress = f64::from(k) / f64::from(steps);
            let offset = f64::from(semitones) * progress / f64::from(BEND_RANGE_SEMITONES);
            let value = (f64::from(BEND_CENTER) + offset * f64::from(BEND_CENTER)).round();
            let tick = start + length * k / steps;
            (tick, value.clamp(0.0, 16383.0) as u16)
        })
        .collect()
}

/// Converts a position to samples for a given tempo and sample rate.
pub fn tick_to_sample(tick: u32, tempo_bpm: u16, sample_rate: u32) -> u64 {
    let seconds_per_tick = 60.0 / (f64::from(tempo_bpm.max(1)) * TICKS_PER_QUARTER);
    (f64::from(tick) * seconds_per_tick * f64::from(sample_rate)).round() as u64
}

/// Inverse of [`tick_to_sample`], fractional so that a cursor can move smoothly.
pub fn sample_to_tick(sample: u64, tempo_bpm: u16, sample_rate: u32) -> f64 {
    let ticks_per_second = f64::from(tempo_bpm.max(1)) * TICKS_PER_QUARTER / 60.0;
    sample as f64 / f64::from(sample_rate.max(1)) * ticks_per_second
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
            notes: vec![Note::fretted(string, fret)],
        }
    }

    /// (tick, is note-on, pitch) of the notes, leaving pitch bends out.
    fn note_events(events: &[Event]) -> Vec<(u32, bool, u8)> {
        events
            .iter()
            .filter_map(|e| match e.action {
                Action::NoteOn { pitch, .. } => Some((e.tick, true, pitch)),
                Action::NoteOff { pitch, .. } => Some((e.tick, false, pitch)),
                Action::PitchBend(_) => None,
            })
            .collect()
    }

    fn bends(events: &[Event]) -> Vec<(u32, u16)> {
        events
            .iter()
            .filter_map(|e| match e.action {
                Action::PitchBend(value) => Some((e.tick, value)),
                _ => None,
            })
            .collect()
    }

    fn velocities(events: &[Event]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|e| match e.action {
                Action::NoteOn { velocity, .. } => Some(velocity),
                _ => None,
            })
            .collect()
    }

    fn with_technique(string: u8, fret: u8, technique: Technique) -> Beat {
        let mut beat = quarter_note(string, fret);
        beat.notes[0].technique = Some(technique);
        beat
    }

    fn score_with(beats: Vec<Beat>) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            tracks: vec![Track {
                name: "g".into(),
                instrument: Instrument::Guitar,
                sound: Sound::GuitarNylon,
                tuning: GUITAR_STANDARD.to_vec(),
                measures: vec![Measure {
                    time_signature: TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    },
                    voices: vec![Voice { beats }],
                }],
                clef: None,
            }],
        }
    }

    #[test]
    fn repeated_pitch_is_retriggered() {
        let events = schedule(&score_with(vec![quarter_note(1, 0), quarter_note(1, 0)]));
        // on@0, off@32, on@32, off@64: the note-off comes before the re-attack.
        assert_eq!(
            note_events(&events),
            vec![
                (0, true, 64),
                (32, false, 64),
                (32, true, 64),
                (64, false, 64)
            ]
        );
    }

    #[test]
    fn a_pitched_track_plays_its_pitches_and_holds_its_ties() {
        let mut score = score_with(vec![]);
        let track = &mut score.tracks[0];
        (track.instrument, track.sound) = (Instrument::Piano, Sound::Piano);
        track.tuning.clear();
        let half = |notes| Beat {
            duration: Duration {
                value: NoteValue::Half,
                dots: 0,
            },
            notes,
        };
        let held = Note {
            tied: true,
            ..Note::pitched(48)
        };
        track.measures[0].voices[0].beats = vec![
            half(vec![Note::pitched(48), Note::pitched(64)]),
            half(vec![held, Note::pitched(67)]),
        ];
        assert_eq!(
            note_events(&schedule(&score)),
            vec![
                (0, true, 48),
                (0, true, 64),
                (64, false, 64),
                (64, true, 67),
                (128, false, 48),
                (128, false, 67)
            ]
        );
    }

    #[test]
    fn a_drum_hit_rings_until_its_piece_is_struck_again_or_the_track_ends() {
        let mut score = score_with(vec![]);
        let track = &mut score.tracks[0];
        (track.instrument, track.sound) = (Instrument::Drums, Sound::DrumsStandard);
        track.tuning.clear();
        let eighth = |notes| Beat {
            duration: Duration {
                value: NoteValue::Eighth,
                dots: 0,
            },
            notes,
        };
        let rest = Beat {
            duration: Duration {
                value: NoteValue::Half,
                dots: 1,
            },
            notes: vec![],
        };
        let tied_crash = Note {
            tied: true,
            // A technique on a drum note means nothing; it must not bend the kit.
            technique: Some(Technique::Bend),
            ..Note::percussion(49)
        };
        track.measures[0].voices[0].beats = vec![
            eighth(vec![Note::percussion(36), Note::percussion(49)]),
            eighth(vec![Note::percussion(36), tied_crash]),
            rest,
        ];
        let events = schedule(&score);
        assert_eq!(
            note_events(&events),
            vec![
                (0, true, 36),
                (0, true, 49),
                (16, false, 36),
                (16, true, 36),
                (128, false, 36),
                (128, false, 49)
            ]
        );
        assert!(bends(&events).is_empty());
        assert!(velocities(&events).iter().all(|v| *v == DEFAULT_VELOCITY));
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
        // String 2 is B3 (59), plus fret 1.
        assert_eq!(note_events(&events), vec![(32, true, 60), (64, false, 60)]);
    }

    #[test]
    fn tied_note_extends_the_previous_one_across_measures() {
        let tied = |tied| Beat {
            duration: Duration {
                value: NoteValue::Whole,
                dots: 0,
            },
            notes: vec![Note {
                technique: None,
                tied,
                ..Note::fretted(1, 0)
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

        let seq: Vec<(u32, bool)> = note_events(&schedule(&score))
            .iter()
            .map(|&(tick, on, _)| (tick, on))
            .collect();
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
                technique: None,
                tied,
                ..Note::fretted(1, fret)
            }],
        };
        let events = schedule(&score_with(vec![beat(0, false), beat(2, true)]));
        assert_eq!(note_events(&events).iter().filter(|e| e.1).count(), 2);
    }

    #[test]
    fn sample_to_tick_inverts_tick_to_sample() {
        let sample = tick_to_sample(96, 100, 48_000);
        assert!((sample_to_tick(sample, 100, 48_000) - 96.0).abs() < 0.01);
    }

    #[test]
    fn tempo_conversion() {
        // 120 bpm: a quarter note = 0.5 s = 22050 samples at 44.1 kHz.
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
        let score = io_midi::import_midi(&bytes).unwrap().score;
        let events = note_events(&schedule(&score));
        assert_eq!(events, vec![(0, true, 64), (32, false, 64)]);
    }

    #[test]
    fn hammer_on_and_pull_off_are_played_softly() {
        let events = schedule(&score_with(vec![
            quarter_note(1, 0),
            with_technique(1, 2, Technique::HammerOn),
            with_technique(1, 0, Technique::PullOff),
        ]));
        let velocities = velocities(&events);
        assert_eq!(velocities[0], DEFAULT_VELOCITY);
        assert!(velocities[1] < DEFAULT_VELOCITY && velocities[2] < DEFAULT_VELOCITY);
    }

    #[test]
    fn a_slide_glides_to_the_next_note_and_lands_softly() {
        let events = schedule(&score_with(vec![
            with_technique(1, 0, Technique::Slide),
            quarter_note(1, 4),
        ]));

        let bends = bends(&events);
        // The glide starts halfway through the first quarter (tick 16) and ends with it (tick 32)
        // four semitones up, then the wheel is put back before the next note.
        assert_eq!(bends.first().map(|b| b.0 > 16), Some(true));
        let top = bends
            .iter()
            .filter(|b| b.0 == 32)
            .map(|b| b.1)
            .collect::<Vec<_>>();
        let four_up =
            BEND_CENTER + (BEND_CENTER as f64 * 4.0 / BEND_RANGE_SEMITONES as f64).round() as u16;
        assert_eq!(top, vec![four_up, BEND_CENTER]);
        assert!(bends.windows(2).all(|w| w[0].0 <= w[1].0));

        // The wheel is back at the centre before the target note starts.
        let order: Vec<_> = events
            .iter()
            .filter(|e| e.tick == 32)
            .map(|e| e.action.order())
            .collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(velocities(&events), vec![DEFAULT_VELOCITY, LEGATO_VELOCITY]);
    }

    #[test]
    fn a_slide_with_nothing_to_reach_is_a_plain_note() {
        let alone = schedule(&score_with(vec![with_technique(1, 0, Technique::Slide)]));
        assert!(bends(&alone).is_empty());

        // Next note on another string, or the same pitch, or too far: no glide either.
        for next in [quarter_note(2, 0), quarter_note(1, 0), quarter_note(1, 24)] {
            let events = schedule(&score_with(vec![
                with_technique(1, 0, Technique::Slide),
                next,
            ]));
            assert!(bends(&events).is_empty());
        }
    }

    #[test]
    fn a_bend_rises_a_whole_tone_and_is_released_with_the_note() {
        let events = schedule(&score_with(vec![with_technique(1, 5, Technique::Bend)]));
        let bends = bends(&events);
        let whole_tone =
            BEND_CENTER + (BEND_CENTER as f64 * 2.0 / BEND_RANGE_SEMITONES as f64).round() as u16;
        assert_eq!(bends.iter().map(|b| b.1).max(), Some(whole_tone));
        assert_eq!(bends.last(), Some(&(32, BEND_CENTER)));
        assert!(bends.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    #[test]
    fn a_palm_mute_is_quieter_and_shorter() {
        let events = schedule(&score_with(vec![with_technique(1, 0, Technique::PalmMute)]));
        assert_eq!(note_events(&events), vec![(0, true, 64), (16, false, 64)]);
        assert!(velocities(&events)[0] < DEFAULT_VELOCITY);
    }

    /// The timbre of the note-on and of the note-off of the first note.
    fn timbres(events: &[Event]) -> Vec<Timbre> {
        events
            .iter()
            .filter_map(|e| match e.action {
                Action::NoteOn { timbre, .. } | Action::NoteOff { timbre, .. } => Some(timbre),
                Action::PitchBend(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_technique_picks_the_timbre_of_both_ends_of_the_note() {
        let timbres_of = |technique| {
            timbres(&schedule(&score_with(vec![with_technique(
                1, 5, technique,
            )])))
        };
        assert_eq!(
            timbres_of(Technique::PalmMute),
            vec![Timbre::Muted, Timbre::Muted]
        );
        assert_eq!(
            timbres_of(Technique::Harmonic),
            vec![Timbre::Harmonic, Timbre::Harmonic]
        );
        assert_eq!(
            timbres_of(Technique::HammerOn),
            vec![Timbre::Legato, Timbre::Legato]
        );
        assert_eq!(
            timbres_of(Technique::Bend),
            vec![Timbre::Open, Timbre::Open]
        );
    }

    #[test]
    fn a_natural_harmonic_sounds_at_the_harmonic_pitch() {
        let at = |fret| {
            let events = schedule(&score_with(vec![with_technique(
                1,
                fret,
                Technique::Harmonic,
            )]));
            note_events(&events)[0].2
        };
        // High E string is 64: the harmonics are an octave, an octave and a fifth, two octaves.
        assert_eq!((at(12), at(7), at(5)), (76, 83, 88));
    }

    #[test]
    fn a_tie_carries_a_bent_note_and_its_release() {
        let mut bent = with_technique(1, 5, Technique::Bend);
        bent.notes[0].technique = Some(Technique::Bend);
        let mut tied = quarter_note(1, 5);
        tied.notes[0].tied = true;
        let events = schedule(&score_with(vec![bent, tied]));

        assert_eq!(note_events(&events), vec![(0, true, 69), (64, false, 69)]);
        assert_eq!(bends(&events).last(), Some(&(64, BEND_CENTER)));
    }
}
