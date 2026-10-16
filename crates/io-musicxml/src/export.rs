//! Export a `Score` as a MusicXML `score-partwise` file.
//!
//! Each track of a string instrument is a part with one tablature staff (`TAB` clef, tuning,
//! string and fret on every note), so other programs show the tablature and can play it. A
//! pitched track (keyboards, synthesizer, voice) is a part with one standard staff: a voice in its
//! own clef, another track in the treble or bass clef that suits its notes best. A drum track is a
//! part on a percussion staff and MIDI channel 10, with an
//! instrument for each piece it plays (`midi-unpitched`), and `unpitched` notes written at the
//! place and with the head of their piece (`display-step`, `display-octave`, `notehead`).
//! Durations are written in divisions of 32 per quarter note, which is exactly the unit of the
//! score: nothing is rounded.

use std::fmt::Write;

use score_model::{
    drum_piece, is_played_by_foot, Beat, Clef, Note, NoteHead, NoteKind, Score, Technique, Track,
    DRUM_PIECES,
};

const DIVISIONS: u32 = 32;
const PERCUSSION_CHANNEL: usize = 9;
const NAMES: [(&str, i32); 12] = [
    ("C", 0),
    ("C", 1),
    ("D", 0),
    ("D", 1),
    ("E", 0),
    ("F", 0),
    ("F", 1),
    ("G", 0),
    ("G", 1),
    ("A", 0),
    ("A", 1),
    ("B", 0),
];

/// The score as a MusicXML document.
pub fn export_musicxml(score: &Score) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = write_score(&mut out, score);
    out
}

fn write_score(out: &mut String, score: &Score) -> std::fmt::Result {
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        out,
        r#"<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">"#
    )?;
    writeln!(out, r#"<score-partwise version="4.0">"#)?;
    writeln!(
        out,
        "  <work><work-title>{}</work-title></work>",
        escape(&score.title)
    )?;
    writeln!(
        out,
        "  <identification><encoding><software>Music Part</software></encoding></identification>"
    )?;

    writeln!(out, "  <part-list>")?;
    for (index, track) in score.tracks.iter().enumerate() {
        let channel = {
            let c = index % 15;
            if c >= PERCUSSION_CHANNEL {
                c + 1
            } else {
                c
            }
        };
        // MusicXML counts programs from 1.
        let program = track.sound.midi_program() + 1;
        if track.sound.is_drum_kit() {
            write_drum_part_info(out, track, index + 1, program)?;
            continue;
        }
        writeln!(
            out,
            r#"    <score-part id="P{n}"><part-name>{name}</part-name><score-instrument id="P{n}-I1"><instrument-name>{name}</instrument-name></score-instrument><midi-instrument id="P{n}-I1"><midi-channel>{channel}</midi-channel><midi-program>{program}</midi-program></midi-instrument></score-part>"#,
            n = index + 1,
            name = escape(&track.name),
            channel = channel + 1,
        )?;
    }
    writeln!(out, "  </part-list>")?;

    for (index, track) in score.tracks.iter().enumerate() {
        write_part(out, score, track, index)?;
    }
    writeln!(out, "</score-partwise>")
}

/// The part of a drum track in the part list: the kit on channel 10 (its program), then an
/// instrument for each piece the track plays, which its notes name.
fn write_drum_part_info(
    out: &mut String,
    track: &Track,
    n: usize,
    program: u8,
) -> std::fmt::Result {
    let name = escape(&track.name);
    let mut pieces: Vec<u8> = notes_of(track)
        .filter_map(|note| track.pitch(note))
        .collect();
    pieces.sort_unstable();
    pieces.dedup();
    write!(
        out,
        r#"    <score-part id="P{n}"><part-name>{name}</part-name><score-instrument id="P{n}-I1"><instrument-name>{name}</instrument-name></score-instrument>"#
    )?;
    for &key in &pieces {
        let piece = drum_piece(key).map_or("", |p| p.name);
        write!(
            out,
            r#"<score-instrument id="P{n}-I{key}"><instrument-name>{piece}</instrument-name></score-instrument>"#
        )?;
    }
    write!(
        out,
        r#"<midi-instrument id="P{n}-I1"><midi-channel>10</midi-channel><midi-program>{program}</midi-program></midi-instrument>"#
    )?;
    for &key in &pieces {
        write!(
            out,
            r#"<midi-instrument id="P{n}-I{key}"><midi-channel>10</midi-channel><midi-program>{program}</midi-program><midi-unpitched>{unpitched}</midi-unpitched></midi-instrument>"#,
            unpitched = key + 1,
        )?;
    }
    writeln!(out, "</score-part>")
}

/// Every note of every voice of a track.
fn notes_of(track: &Track) -> impl Iterator<Item = &Note> {
    track
        .measures
        .iter()
        .flat_map(|m| &m.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
}

fn write_part(out: &mut String, score: &Score, track: &Track, index: usize) -> std::fmt::Result {
    writeln!(out, r#"  <part id="P{}">"#, index + 1)?;
    let voices = track
        .measures
        .iter()
        .map(|m| m.voices.len())
        .max()
        .unwrap_or(1);
    let mut previous_signature = None;

    for (m, measure) in track.measures.iter().enumerate() {
        writeln!(out, r#"    <measure number="{}">"#, m + 1)?;
        let signature = measure.time_signature;
        if m == 0 || previous_signature != Some(signature) {
            write!(out, "      <attributes>")?;
            if m == 0 {
                write!(
                    out,
                    "<divisions>{DIVISIONS}</divisions><key><fifths>0</fifths></key>"
                )?;
            }
            write!(
                out,
                "<time><beats>{}</beats><beat-type>{}</beat-type></time>",
                signature.numerator, signature.denominator
            )?;
            let kind = track.instrument.note_kind();
            if m == 0 && kind == NoteKind::Pitched {
                write!(out, "{}", clef_for(track))?;
            } else if m == 0 && kind == NoteKind::Percussion {
                write!(out, "<clef><sign>percussion</sign></clef>")?;
            } else if m == 0 {
                write!(out, "<clef><sign>TAB</sign><line>5</line></clef>")?;
                write!(
                    out,
                    "<staff-details><staff-lines>{}</staff-lines>",
                    track.tuning.len()
                )?;
                // Line 1 is the lowest string; the score lists the highest first.
                for (line, pitch) in track.tuning.iter().rev().enumerate() {
                    let (step, alter, octave) = spell(*pitch);
                    write!(
                        out,
                        "<staff-tuning line=\"{}\"><tuning-step>{step}</tuning-step>{}<tuning-octave>{octave}</tuning-octave></staff-tuning>",
                        line + 1,
                        if alter != 0 {
                            format!("<tuning-alter>{alter}</tuning-alter>")
                        } else {
                            String::new()
                        }
                    )?;
                }
                write!(out, "</staff-details>")?;
            }
            writeln!(out, "</attributes>")?;
            previous_signature = Some(signature);
        }
        if m == 0 && index == 0 {
            writeln!(
                out,
                r#"      <direction placement="above"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{bpm}</per-minute></metronome></direction-type><sound tempo="{bpm}"/></direction>"#,
                bpm = score.tempo_bpm
            )?;
        }

        for voice in 0..voices {
            let Some(content) = measure.voices.get(voice) else {
                continue;
            };
            if voice > 0 {
                writeln!(
                    out,
                    "      <backup><duration>{}</duration></backup>",
                    signature.ticks()
                )?;
            }
            for (b, beat) in content.beats.iter().enumerate() {
                let around = Around {
                    before: previous_beat(track, m, voice, b),
                    after: next_beat(track, m, voice, b),
                };
                let part = index + 1;
                write_beat(out, track, beat, (part, voice + 1), &around)?;
            }
        }
        writeln!(out, "    </measure>")?;
    }
    writeln!(out, "  </part>")
}

/// The beats on either side of one, in the same voice, across barlines.
struct Around<'a> {
    before: Option<&'a Beat>,
    after: Option<&'a Beat>,
}

fn previous_beat(track: &Track, measure: usize, voice: usize, beat: usize) -> Option<&Beat> {
    if let Some(i) = beat.checked_sub(1) {
        return track.measures[measure].voices.get(voice)?.beats.get(i);
    }
    track.measures[..measure]
        .iter()
        .rev()
        .find_map(|m| m.voices.get(voice))?
        .beats
        .last()
}

fn next_beat(track: &Track, measure: usize, voice: usize, beat: usize) -> Option<&Beat> {
    if let Some(next) = track.measures[measure]
        .voices
        .get(voice)?
        .beats
        .get(beat + 1)
    {
        return Some(next);
    }
    track.measures[measure + 1..]
        .iter()
        .find_map(|m| m.voices.get(voice))?
        .beats
        .first()
}

fn write_beat(
    out: &mut String,
    track: &Track,
    beat: &Beat,
    (part, voice): (usize, usize),
    around: &Around,
) -> std::fmt::Result {
    let duration = beat.duration.ticks();
    let (kind, dots) = (
        note_type(beat),
        "<dot/>".repeat(usize::from(beat.duration.dots)),
    );

    if beat.notes.is_empty() {
        return writeln!(
            out,
            "      <note><rest/><duration>{duration}</duration><voice>{voice}</voice><type>{kind}</type>{dots}</note>"
        );
    }
    for (i, note) in beat.notes.iter().enumerate() {
        let Some(pitch) = track.pitch(note) else {
            continue;
        };
        let next = same_note(around.after, note);
        let previous = same_note(around.before, note);

        let continues = next.is_some_and(|n| n.tied);
        let mut ties = String::new();
        let mut tied = String::new();
        if note.tied {
            ties.push_str(r#"<tie type="stop"/>"#);
            tied.push_str(r#"<tied type="stop"/>"#);
        }
        if continues {
            ties.push_str(r#"<tie type="start"/>"#);
            tied.push_str(r#"<tied type="start"/>"#);
        }

        write!(out, "      <note>")?;
        if i > 0 {
            write!(out, "<chord/>")?;
        }
        if let Some(piece) = DRUM_PIECES
            .contains(&pitch)
            .then(|| drum_piece(pitch))
            .flatten()
            .filter(|_| track.instrument.note_kind() == NoteKind::Percussion)
        {
            // Stems up for the hands, down for a beat of feet only, as drummers read them.
            let stem = if beat.notes.iter().all(|n| is_played_by_foot(n.key())) {
                "down"
            } else {
                "up"
            };
            let head = match piece.head {
                NoteHead::Normal => "",
                NoteHead::Cross => "<notehead>x</notehead>",
                NoteHead::CircleCross => "<notehead>circle-x</notehead>",
                NoteHead::Diamond => "<notehead>diamond</notehead>",
                NoteHead::Triangle => "<notehead>triangle</notehead>",
            };
            write!(
                out,
                r#"<unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched><duration>{duration}</duration>{ties}<instrument id="P{part}-I{pitch}"/><voice>{voice}</voice><type>{kind}</type>{dots}<stem>{stem}</stem>{head}"#,
                step = piece.step,
                octave = piece.octave,
            )?;
        } else {
            let (step, alter, octave) = spell(pitch);
            write!(out, "<pitch><step>{step}</step>")?;
            if alter != 0 {
                write!(out, "<alter>{alter}</alter>")?;
            }
            write!(
                out,
                "<octave>{octave}</octave></pitch><duration>{duration}</duration>{ties}<voice>{voice}</voice><type>{kind}</type>{dots}"
            )?;
        }
        write!(out, "<notations>{tied}")?;
        write_slide(out, note, previous)?;
        if let (Some(string), Some(fret)) = (note.string(), note.fret()) {
            write!(
                out,
                "<technical><string>{string}</string><fret>{fret}</fret>"
            )?;
            write_technique(out, note, next)?;
            write!(out, "</technical>")?;
        }
        writeln!(out, "</notations></note>")?;
    }
    Ok(())
}

/// The note of a neighbouring beat on the same string, or at the same pitch on a pitched track.
fn same_note<'a>(beat: Option<&'a Beat>, note: &Note) -> Option<&'a Note> {
    beat?.notes.iter().find(|n| n.key() == note.key())
}

const TREBLE: &str = "<clef><sign>G</sign><line>2</line></clef>";
const BASS: &str = "<clef><sign>F</sign><line>4</line></clef>";

/// The clef of the track when it has one (a voice); else the treble clef, or the bass clef when
/// most notes of the track are below middle C. Notes are written at their sounding pitch either
/// way: the 8 of a tenor's clef is a `clef-octave-change`, not a transposition.
fn clef_for(track: &Track) -> &'static str {
    match track.clef {
        Some(Clef::Treble) => return TREBLE,
        Some(Clef::Treble8vb) => {
            return "<clef><sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change></clef>"
        }
        Some(Clef::Bass) => return BASS,
        None => {}
    }
    let (low, high) = track
        .measures
        .iter()
        .flat_map(|m| &m.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
        .fold((0usize, 0usize), |(low, high), n| {
            if n.key() < 60 {
                (low + 1, high)
            } else {
                (low, high + 1)
            }
        });
    if low > high {
        BASS
    } else {
        TREBLE
    }
}

fn write_slide(out: &mut String, note: &Note, previous: Option<&Note>) -> std::fmt::Result {
    if previous.and_then(|p| p.technique) == Some(Technique::Slide) {
        write!(out, r#"<slide type="stop" number="1"/>"#)?;
    }
    if note.technique == Some(Technique::Slide) {
        write!(out, r#"<slide type="start" number="1"/>"#)?;
    }
    Ok(())
}

/// A hammer-on or pull-off is written on both notes: it starts on the one before and stops on
/// this one. `next` is the note of the same string in the following beat.
fn write_technique(out: &mut String, note: &Note, next: Option<&Note>) -> std::fmt::Result {
    match note.technique {
        Some(Technique::HammerOn) => write!(out, r#"<hammer-on type="stop" number="1"/>"#)?,
        Some(Technique::PullOff) => write!(out, r#"<pull-off type="stop" number="1"/>"#)?,
        Some(Technique::Bend) => write!(out, "<bend><bend-alter>2</bend-alter></bend>")?,
        Some(Technique::Harmonic) => write!(out, "<harmonic><natural/></harmonic>")?,
        Some(Technique::PalmMute) => write!(out, "<other-technical>palm mute</other-technical>")?,
        Some(Technique::Slide) | None => {}
    }
    match next.and_then(|n| n.technique) {
        Some(Technique::HammerOn) => {
            write!(out, r#"<hammer-on type="start" number="1">H</hammer-on>"#)?
        }
        Some(Technique::PullOff) => {
            write!(out, r#"<pull-off type="start" number="1">P</pull-off>"#)?
        }
        _ => {}
    }
    Ok(())
}

fn note_type(beat: &Beat) -> &'static str {
    use score_model::NoteValue::*;
    match beat.duration.value {
        Whole => "whole",
        Half => "half",
        Quarter => "quarter",
        Eighth => "eighth",
        Sixteenth => "16th",
        ThirtySecond => "32nd",
    }
}

/// Step, alteration (sharps) and octave of a MIDI pitch. Middle C (60) is C4.
fn spell(pitch: u8) -> (&'static str, i32, i32) {
    let (step, alter) = NAMES[usize::from(pitch % 12)];
    (step, alter, i32::from(pitch / 12) - 1)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters cannot appear in XML 1.0 at all.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_musicxml;
    use score_model::{
        apply_edit, blank_score, demo_score, Clef, Edit, Instrument, Sound, Technique,
    };

    /// What a round trip may change without changing the music: the order of the notes of a chord,
    /// and silent extra voices (the importer gives every measure of a part the same voices).
    fn normalized(score: &Score) -> Score {
        let mut score = score.clone();
        for track in &mut score.tracks {
            for measure in &mut track.measures {
                for voice in &mut measure.voices {
                    for beat in &mut voice.beats {
                        beat.notes.sort_by_key(Note::key);
                    }
                }
                let mut index = 0;
                measure.voices.retain(|voice| {
                    index += 1;
                    index == 1 || voice.beats.iter().any(|b| !b.notes.is_empty())
                });
            }
        }
        score
    }

    fn assert_same_music(a: &Score, b: &Score) {
        let (a, b) = (normalized(a), normalized(b));
        assert_eq!(a.title, b.title);
        assert_eq!(a.tempo_bpm, b.tempo_bpm);
        assert_eq!(a.tracks.len(), b.tracks.len());
        for (x, y) in a.tracks.iter().zip(&b.tracks) {
            assert_eq!(
                (&x.name, x.instrument, x.sound, &x.tuning, x.clef),
                (&y.name, y.instrument, y.sound, &y.tuning, y.clef)
            );
            assert_eq!(x.measures.len(), y.measures.len());
            for (m, (p, q)) in x.measures.iter().zip(&y.measures).enumerate() {
                assert_eq!(p, q, "track {} measure {}", x.name, m + 1);
            }
        }
    }

    fn roundtrip(score: &Score) -> Score {
        let xml = export_musicxml(score);
        let imported = import_musicxml(xml.as_bytes()).expect("the export can be imported");
        assert_eq!(imported.skipped_notes, 0);
        imported.score
    }

    #[test]
    fn the_demo_survives_a_round_trip_exactly() {
        let original = demo_score();
        assert_same_music(&roundtrip(&original), &original);
    }

    #[test]
    fn every_sound_survives_a_round_trip() {
        let mut score = blank_score("t");
        for sound in Sound::ALL {
            apply_edit(
                &mut score,
                &Edit::AddTrack {
                    instrument: sound.instrument(),
                    sound: Some(sound),
                },
            )
            .unwrap();
            let track = score.tracks.len() - 1;
            // The last string at fret 7: on a banjo, the short fifth string. A chord on a piano.
            let string = score.tracks[track].tuning.len() as u8;
            let edits = if string == 0 {
                [48, 61, 67]
                    .map(|pitch| Edit::AddPitch {
                        track,
                        measure: 0,
                        beat: 0,
                        pitch,
                    })
                    .to_vec()
            } else {
                vec![Edit::SetFret {
                    track,
                    measure: 0,
                    beat: 0,
                    string,
                    fret: 7,
                }]
            };
            for edit in edits {
                apply_edit(&mut score, &edit).unwrap();
            }
        }
        assert_same_music(&roundtrip(&score), &score);
    }

    #[test]
    fn a_voice_keeps_its_clef_even_when_its_range_would_suggest_another() {
        let mut score = blank_score("Choir");
        for clef in Clef::ALL {
            let edits = [
                Edit::AddTrack {
                    instrument: Instrument::Voice,
                    sound: Some(Sound::VoiceOohs),
                },
                Edit::SetClef {
                    track: score.tracks.len(),
                    clef,
                },
                // A4: the range of a soprano, whatever the clef says.
                Edit::AddPitch {
                    track: score.tracks.len(),
                    measure: 0,
                    beat: 0,
                    pitch: 69,
                },
            ];
            for edit in edits {
                apply_edit(&mut score, &edit).unwrap();
            }
        }
        apply_edit(&mut score, &Edit::RemoveTrack { index: 0 }).unwrap();
        let xml = export_musicxml(&score);
        assert!(xml.contains("<clef-octave-change>-1</clef-octave-change>"));
        // Pitches are written as they sound, in every clef.
        assert_eq!(xml.matches("<octave>4</octave>").count(), 3);
        let back = roundtrip(&score);
        let clefs: Vec<_> = back.tracks.iter().map(|t| t.clef).collect();
        assert_eq!(clefs, Clef::ALL.map(Some));
        assert_same_music(&back, &score);
    }

    #[test]
    fn a_low_piano_part_is_written_in_the_bass_clef_with_its_ties() {
        let mut score = blank_score("Low");
        let edits = [
            Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Piano,
            },
            // A part named after a string instrument would be read back as one.
            Edit::RenameTrack {
                track: 0,
                name: "Left hand".into(),
            },
            Edit::AddPitch {
                track: 0,
                measure: 0,
                beat: 0,
                pitch: 40,
            },
            Edit::AddPitch {
                track: 0,
                measure: 1,
                beat: 0,
                pitch: 40,
            },
            Edit::SetPitchTied {
                track: 0,
                measure: 1,
                beat: 0,
                pitch: 40,
                tied: true,
            },
        ];
        for edit in edits {
            apply_edit(&mut score, &edit).unwrap();
        }
        let xml = export_musicxml(&score);
        assert!(xml.contains("<clef><sign>F</sign><line>4</line></clef>"));
        assert!(!xml.contains("<technical>"), "no string or fret on a piano");
        assert_same_music(&roundtrip(&score), &score);
    }

    #[test]
    fn ties_techniques_and_voices_survive_a_round_trip() {
        let mut score = blank_score("Title & <other>");
        for edit in [
            Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 0,
                string: 2,
                fret: 3,
            },
            Edit::SetDuration {
                track: 0,
                measure: 0,
                beat: 0,
                duration: score_model::Duration {
                    value: score_model::NoteValue::Half,
                    dots: 0,
                },
            },
            Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 1,
                string: 2,
                fret: 3,
            },
            Edit::SetTied {
                track: 0,
                measure: 0,
                beat: 1,
                string: 2,
                tied: true,
            },
            Edit::SetFret {
                track: 0,
                measure: 1,
                beat: 0,
                string: 4,
                fret: 0,
            },
            Edit::SetTechnique {
                track: 0,
                measure: 1,
                beat: 0,
                string: 4,
                technique: Some(Technique::Bend),
            },
            Edit::SetFret {
                track: 0,
                measure: 2,
                beat: 0,
                string: 1,
                fret: 7,
            },
            Edit::SetTechnique {
                track: 0,
                measure: 2,
                beat: 0,
                string: 1,
                technique: Some(Technique::PalmMute),
            },
            Edit::SetTimeSignature {
                measure: 3,
                numerator: 3,
                denominator: 4,
            },
            Edit::RenameTrack {
                track: 0,
                name: "Lead \"one\"".into(),
            },
            Edit::AddTrack {
                instrument: Instrument::Violin,
                sound: None,
            },
            Edit::SetTempo { bpm: 77 },
        ] {
            apply_edit(&mut score, &edit).unwrap();
        }
        // A second voice in the first measure.
        score.tracks[0].measures[0].voices.push(score_model::Voice {
            beats: vec![Beat {
                duration: score_model::Duration {
                    value: score_model::NoteValue::Whole,
                    dots: 0,
                },
                notes: vec![Note {
                    technique: Some(Technique::Harmonic),
                    ..Note::fretted(5, 2)
                }],
            }],
        });
        assert_eq!(score.validate(), Ok(()));

        assert_same_music(&roundtrip(&score), &score);
    }

    #[test]
    fn techniques_that_link_two_notes_are_written_on_both() {
        let mut score = blank_score("t");
        for edit in [
            Edit::SetDuration {
                track: 0,
                measure: 0,
                beat: 0,
                duration: score_model::Duration {
                    value: score_model::NoteValue::Half,
                    dots: 0,
                },
            },
            Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 0,
                string: 1,
                fret: 0,
            },
            Edit::SetFret {
                track: 0,
                measure: 0,
                beat: 1,
                string: 1,
                fret: 2,
            },
            Edit::SetTechnique {
                track: 0,
                measure: 0,
                beat: 1,
                string: 1,
                technique: Some(Technique::HammerOn),
            },
            Edit::SetTechnique {
                track: 0,
                measure: 0,
                beat: 0,
                string: 1,
                technique: Some(Technique::Slide),
            },
        ] {
            apply_edit(&mut score, &edit).unwrap();
        }
        let xml = export_musicxml(&score);
        assert!(
            xml.contains(r#"<hammer-on type="start""#) && xml.contains(r#"<hammer-on type="stop""#)
        );
        assert!(xml.contains(r#"<slide type="start""#) && xml.contains(r#"<slide type="stop""#));
    }

    /// A drum track with one quarter note on each piece, on the given kit.
    fn every_piece(sound: Sound) -> Score {
        let mut score = blank_score("Drums");
        apply_edit(
            &mut score,
            &Edit::SetInstrument {
                track: 0,
                instrument: Instrument::Drums,
            },
        )
        .unwrap();
        apply_edit(&mut score, &Edit::SetSound { track: 0, sound }).unwrap();
        let pieces: Vec<u8> = score_model::DRUM_PIECES.collect();
        let measures = pieces.len().div_ceil(4);
        for at in score.tracks[0].measures.len()..measures {
            apply_edit(&mut score, &Edit::InsertMeasure { at }).unwrap();
        }
        let quarter = score_model::Duration {
            value: score_model::NoteValue::Quarter,
            dots: 0,
        };
        for measure in 0..measures {
            for beat in 0..4 {
                let duration = quarter;
                let edit = Edit::SetDuration {
                    track: 0,
                    measure,
                    beat,
                    duration,
                };
                apply_edit(&mut score, &edit).unwrap();
            }
        }
        for (i, &pitch) in pieces.iter().enumerate() {
            let (measure, beat) = (i / 4, i % 4);
            let edit = Edit::AddPitch {
                track: 0,
                measure,
                beat,
                pitch,
            };
            apply_edit(&mut score, &edit).unwrap();
        }
        score
    }

    #[test]
    fn every_drum_piece_survives_a_round_trip() {
        let original = every_piece(Sound::DrumsElectronic);
        let xml = export_musicxml(&original);
        assert!(xml.contains("<sign>percussion</sign>"));
        assert!(xml.contains("<midi-channel>10</midi-channel><midi-program>25</midi-program>"));
        assert!(
            xml.contains(r#"<midi-unpitched>37</midi-unpitched>"#),
            "kick"
        );
        assert!(xml.contains(
            r#"<unpitched><display-step>G</display-step><display-octave>5</display-octave></unpitched>"#
        ));
        assert_same_music(&roundtrip(&original), &original);
    }

    #[test]
    fn drum_notes_are_read_back_from_their_place_when_no_instrument_says_which() {
        let original = every_piece(Sound::DrumsStandard);
        let xml = export_musicxml(&original);
        // Another program may write the place and head but no instrument.
        let without: String = xml
            .lines()
            .map(|line| {
                let mut line = line.to_string();
                while let Some(start) = line.find("<instrument id=") {
                    let end = start + line[start..].find("/>").unwrap() + 2;
                    line.replace_range(start..end, "");
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!without.contains("<instrument id="));
        let imported = import_musicxml(without.as_bytes()).unwrap();
        assert_eq!(imported.skipped_notes, 0);
        assert_same_music(&imported.score, &original);
    }

    #[test]
    fn a_beat_of_feet_only_has_its_stem_down() {
        let mut score = every_piece(Sound::DrumsStandard);
        // Beat 2 holds the kick (36) alone; add the hi-hat to beat 1, with the acoustic kick.
        apply_edit(
            &mut score,
            &Edit::AddPitch {
                track: 0,
                measure: 0,
                beat: 0,
                pitch: 42,
            },
        )
        .unwrap();
        let xml = export_musicxml(&score);
        let stems: Vec<&str> = xml
            .lines()
            .filter(|l| l.contains("<unpitched>"))
            .take(3)
            .map(|l| {
                if l.contains("<stem>down</stem>") {
                    "down"
                } else {
                    "up"
                }
            })
            .collect();
        assert_eq!(stems, ["up", "up", "down"]);
    }

    #[test]
    fn text_is_escaped_so_the_file_stays_well_formed() {
        let mut score = blank_score("A & B <c> \"d\" 'e'\u{1}");
        score.tracks[0].name = "x<y".into();
        let xml = export_musicxml(&score);
        assert!(roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            }
        )
        .is_ok());
        assert_eq!(roundtrip(&score).title, "A & B <c> \"d\" 'e'");
    }

    #[test]
    fn pitches_are_spelled_with_sharps_and_the_octave_of_middle_c() {
        assert_eq!(spell(60), ("C", 0, 4));
        assert_eq!(spell(61), ("C", 1, 4));
        assert_eq!(spell(40), ("E", 0, 2));
        assert_eq!(spell(127), ("G", 0, 9));
    }
}
