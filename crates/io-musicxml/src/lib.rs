//! MusicXML import: reads a `score-partwise` file (`.musicxml`, `.xml`) or a compressed `.mxl`
//! and builds a [`Score`].
//!
//! Positions are quantized to a 32nd note, like the MIDI importer, so triplets and other
//! tuplets are rounded onto the grid and every measure stays exactly full. Notes that give a
//! string and a fret (tablature exports) keep them; the others are placed on the strings by
//! pitch. Repeats and endings are not unrolled: every measure is read once.
//!
//! Like every importer, it never panics on a bad file: it returns an error, or leaves notes out
//! and counts them in [`MusicXmlImport::skipped_notes`].

mod export;

pub use export::export_musicxml;

use std::collections::HashMap;
use std::fmt;
use std::io::{Cursor, Read};

use roxmltree::{Document, Node, ParsingOptions};
use score_model::{
    decompose_ticks, fret_pitch, place_chord, Beat, Duration, Instrument, Measure, Note, Score,
    Sound, Technique, TimeSignature, Track, Voice, MAX_FRET,
};

/// Notes are quantized to a 32nd note: 4 units of 1/128 of a whole note.
const GRID: i64 = 4;
/// A quarter note is 32 units.
const TICKS_PER_QUARTER: f64 = 32.0;
const MAX_MEASURES: usize = 5000;
const MAX_PARTS: usize = 64;
/// Voices (counting staves) read per part; the notes of the others are left out.
const MAX_VOICES: usize = 16;
/// Largest XML document read, after decompression for `.mxl`.
const MAX_XML_BYTES: u64 = 128 * 1024 * 1024;
const MAX_XML_NODES: u32 = 8_000_000;
const DEFAULT_TEMPO: u16 = 120;
const MAX_TEMPO: u16 = 999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicXmlImportError {
    /// The XML (or the archive around it) could not be read.
    Parse(String),
    /// Well-formed XML, but not a MusicXML score.
    NotMusicXml,
    /// A MusicXML variant that is not supported, such as `score-timewise`.
    Unsupported(String),
    TooLarge,
    TooLong,
    /// No part with pitched notes or rests to build a track from.
    NoPlayablePart,
    /// The score that was built broke a rule of the model. A bug, or an odd file.
    Invalid(String),
}

impl fmt::Display for MusicXmlImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "cannot read this MusicXML file: {message}"),
            Self::NotMusicXml => write!(f, "this file is not a MusicXML score"),
            Self::Unsupported(what) => write!(f, "{what} is not supported"),
            Self::TooLarge => write!(f, "this MusicXML file is too large"),
            Self::TooLong => write!(f, "this piece has too many measures"),
            Self::NoPlayablePart => write!(f, "this score has no instrument part to import"),
            Self::Invalid(message) => write!(f, "the imported score is not valid: {message}"),
        }
    }
}

impl std::error::Error for MusicXmlImportError {}

pub struct MusicXmlImport {
    pub score: Score,
    /// Notes that could not be placed: no string can play the pitch, grace notes, percussion,
    /// notes in measures that run over, notes in voices beyond the limit.
    pub skipped_notes: usize,
}

/// Imports a MusicXML score, plain or compressed (`.mxl`, detected from the bytes).
pub fn import_musicxml(bytes: &[u8]) -> Result<MusicXmlImport, MusicXmlImportError> {
    if bytes.starts_with(b"PK") {
        parse(&read_archive(bytes)?)
    } else {
        parse(bytes)
    }
}

// ---- compressed files -----------------------------------------------------------------------

fn read_archive(bytes: &[u8]) -> Result<Vec<u8>, MusicXmlImportError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| MusicXmlImportError::Parse(format!("not a valid archive ({e})")))?;

    // The container says which file is the score; without it, take the first XML outside META-INF.
    let from_container = read_entry(&mut archive, "META-INF/container.xml")
        .ok()
        .and_then(|container| {
            let text = decode(&container);
            let doc = Document::parse(&text).ok()?;
            doc.descendants()
                .find(|n| n.has_tag_name("rootfile"))
                .and_then(|n| n.attribute("full-path").map(str::to_string))
        });
    let name = from_container.or_else(|| {
        (0..archive.len()).find_map(|i| {
            let name = archive.name_for_index(i)?.ok()?;
            let lower = name.to_ascii_lowercase();
            let is_score = lower.ends_with(".xml") || lower.ends_with(".musicxml");
            (is_score && !lower.starts_with("meta-inf/")).then(|| name.to_string())
        })
    });
    let name =
        name.ok_or_else(|| MusicXmlImportError::Parse("the archive holds no score".into()))?;
    read_entry(&mut archive, &name)
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<u8>, MusicXmlImportError> {
    let entry = archive
        .by_name(name)
        .map_err(|e| MusicXmlImportError::Parse(format!("cannot open {name} ({e})")))?;
    let mut out = Vec::new();
    // Stop reading at the limit instead of trusting the size the archive claims.
    entry
        .take(MAX_XML_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|e| MusicXmlImportError::Parse(format!("cannot read {name} ({e})")))?;
    if out.len() as u64 > MAX_XML_BYTES {
        return Err(MusicXmlImportError::TooLarge);
    }
    Ok(out)
}

/// UTF-8 (with or without a byte order mark), UTF-16 with a mark, or Latin-1 as a last resort.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], big_endian: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                if big_endian {
                    u16::from_be_bytes([p[0], p[1]])
                } else {
                    u16::from_le_bytes([p[0], p[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, false);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, true);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

// ---- reading the score ----------------------------------------------------------------------

struct PartInfo {
    name: Option<String>,
    midi_program: Option<u8>,
}

fn parse(bytes: &[u8]) -> Result<MusicXmlImport, MusicXmlImportError> {
    if bytes.len() as u64 > MAX_XML_BYTES {
        return Err(MusicXmlImportError::TooLarge);
    }
    let text = decode(bytes);
    // MusicXML files declare a DOCTYPE; the parser still bounds nodes and entity expansion.
    let options = ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_XML_NODES,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(&text, options)
        .map_err(|e| MusicXmlImportError::Parse(e.to_string()))?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "score-partwise" => {}
        "score-timewise" => {
            return Err(MusicXmlImportError::Unsupported(
                "score-timewise MusicXML".into(),
            ))
        }
        _ => return Err(MusicXmlImportError::NotMusicXml),
    }

    let infos = part_infos(root);
    let title = child(root, "work")
        .and_then(|w| child_text(w, "work-title"))
        .or_else(|| child_text(root, "movement-title"))
        .unwrap_or_default();

    let mut skipped_notes = 0;
    let mut tracks: Vec<Track> = Vec::new();
    for part in root
        .children()
        .filter(|n| n.has_tag_name("part"))
        .take(MAX_PARTS)
    {
        let info = part.attribute("id").and_then(|id| infos.get(id));
        if let Some(track) = read_part(part, info, tracks.len(), &mut skipped_notes)? {
            tracks.push(track);
        }
    }
    if tracks.is_empty() {
        return Err(MusicXmlImportError::NoPlayablePart);
    }
    pad_to_same_length(&mut tracks)?;

    let score = Score {
        title,
        tempo_bpm: find_tempo(root),
        tracks,
    };
    score
        .validate()
        .map_err(|e| MusicXmlImportError::Invalid(e.to_string()))?;
    Ok(MusicXmlImport {
        score,
        skipped_notes,
    })
}

fn part_infos(root: Node) -> HashMap<String, PartInfo> {
    let mut infos = HashMap::new();
    let Some(list) = child(root, "part-list") else {
        return infos;
    };
    for part in list.children().filter(|n| n.has_tag_name("score-part")) {
        let Some(id) = part.attribute("id") else {
            continue;
        };
        let midi_program = part
            .descendants()
            .find(|n| n.has_tag_name("midi-program"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<u32>().ok())
            // MusicXML counts programs from 1.
            .and_then(|p| u8::try_from(p.saturating_sub(1)).ok());
        infos.insert(
            id.to_string(),
            PartInfo {
                name: child_text(part, "part-name")
                    .or_else(|| child_text(part, "part-abbreviation")),
                midi_program,
            },
        );
    }
    infos
}

/// The first tempo given by a `sound` element or a metronome mark in quarter notes.
fn find_tempo(root: Node) -> u16 {
    let from_sound = root
        .descendants()
        .filter(|n| n.has_tag_name("sound"))
        .find_map(|n| n.attribute("tempo"))
        .and_then(|t| t.trim().parse::<f64>().ok());
    let from_metronome = || {
        root.descendants()
            .filter(|n| n.has_tag_name("metronome"))
            .find(|n| child_text(*n, "beat-unit").as_deref() == Some("quarter"))
            .and_then(|n| child_text(n, "per-minute"))
            .and_then(|t| t.trim().parse::<f64>().ok())
    };
    match from_sound.or_else(from_metronome) {
        Some(bpm) if bpm.is_finite() && bpm >= 1.0 => {
            (bpm.round() as u32).min(u32::from(MAX_TEMPO)) as u16
        }
        _ => DEFAULT_TEMPO,
    }
}

/// What the part says about its staff, found before reading the notes.
struct PartSetup {
    /// The staff that is tablature, when the part has both notation and tablature staves.
    tab_staff: Option<u32>,
    tuning: Option<Vec<u8>>,
}

fn scan_setup(part: Node) -> PartSetup {
    let tab_staff = part
        .descendants()
        .filter(|n| n.has_tag_name("clef"))
        .find(|n| child_text(*n, "sign").as_deref() == Some("TAB"))
        .map(|n| {
            n.attribute("number")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
        });

    let tuning = part
        .descendants()
        .filter(|n| n.has_tag_name("staff-details"))
        .filter(|n| match (tab_staff, n.attribute("number")) {
            (Some(tab), Some(number)) => number.parse::<u32>().ok() == Some(tab),
            _ => true,
        })
        .find_map(|details| {
            let mut lines: Vec<(u32, u8)> = details
                .children()
                .filter(|n| n.has_tag_name("staff-tuning"))
                .filter_map(|t| {
                    let line = t.attribute("line")?.parse().ok()?;
                    let step = child_text(t, "tuning-step")?;
                    let octave: i32 = child_text(t, "tuning-octave")?.parse().ok()?;
                    let alter = child_text(t, "tuning-alter")
                        .and_then(|a| a.parse::<f64>().ok())
                        .map_or(0, |a| a.round() as i32);
                    let pitch = u8::try_from(pitch_of(&step, alter, octave)?).ok()?;
                    Some((line, pitch))
                })
                .collect();
            if lines.is_empty() || lines.len() > 12 {
                return None;
            }
            // Line 1 is the lowest string; the model lists the highest string first.
            lines.sort_by_key(|&(line, _)| line);
            Some(lines.into_iter().rev().map(|(_, pitch)| pitch).collect())
        });
    PartSetup { tab_staff, tuning }
}

fn pitch_of(step: &str, alter: i32, octave: i32) -> Option<i32> {
    let semitone = match step {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let pitch = (octave + 1) * 12 + semitone + alter;
    (0..=127).contains(&pitch).then_some(pitch)
}

fn sound_for(info: Option<&PartInfo>) -> Sound {
    // A program the file names wins when it is a guitar, bass, violin or banjo; a piano default
    // is not.
    if let Some(program) = info
        .and_then(|i| i.midi_program)
        .filter(|p| (24..=40).contains(p) || *p == Sound::Banjo.midi_program())
    {
        return Sound::from_midi_program(Some(program));
    }
    let name = info
        .and_then(|i| i.name.as_deref())
        .unwrap_or_default()
        .to_lowercase();
    if name.contains("electric") && name.contains("guitar") {
        Instrument::ElectricGuitar.default_sound()
    } else if name.contains("bass") {
        Instrument::Bass.default_sound()
    } else if name.contains("violin") {
        Instrument::Violin.default_sound()
    } else if name.contains("banjo") {
        Instrument::Banjo.default_sound()
    } else if name.contains("steel") {
        Sound::GuitarSteel
    } else {
        Instrument::Guitar.default_sound()
    }
}

/// One note of a measure as written in the file, before it is placed on the grid.
struct RawNote {
    voice: usize,
    /// Start and length in the divisions of the measure.
    start: i64,
    length: i64,
    kind: RawKind,
    is_tie_end: bool,
    technique: Option<Technique>,
}

enum RawKind {
    Rest,
    /// Sounding MIDI pitch, and the string and fret when the file gives them.
    Pitched {
        pitch: Option<u8>,
        fixed: Option<(u8, u8)>,
    },
}

struct RawMeasure {
    signature: TimeSignature,
    /// A pickup or other short measure that should keep its own length.
    is_implicit: bool,
    divisions: f64,
    notes: Vec<RawNote>,
}

fn read_part(
    part: Node,
    info: Option<&PartInfo>,
    index: usize,
    skipped: &mut usize,
) -> Result<Option<Track>, MusicXmlImportError> {
    let setup = scan_setup(part);
    let mut voice_keys: Vec<(u32, String)> = Vec::new();
    let mut divisions = 1.0;
    let mut signature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };
    let mut transpose = 0;
    let mut raw_measures: Vec<RawMeasure> = Vec::new();
    let mut has_pitched = false;
    let mut has_unpitched = false;

    for measure in part.children().filter(|n| n.has_tag_name("measure")) {
        if raw_measures.len() >= MAX_MEASURES {
            return Err(MusicXmlImportError::TooLong);
        }
        let mut position: i64 = 0;
        let mut last_start: i64 = 0;
        let mut notes: Vec<RawNote> = Vec::new();

        for element in measure.children().filter(Node::is_element) {
            match element.tag_name().name() {
                "attributes" => {
                    if let Some(d) =
                        child_text(element, "divisions").and_then(|t| t.parse::<f64>().ok())
                    {
                        if d.is_finite() && d > 0.0 {
                            divisions = d;
                        }
                    }
                    if let Some(time) = child(element, "time") {
                        if let Some(read) = read_signature(time) {
                            signature = read;
                        }
                    }
                    if let Some(t) = child(element, "transpose") {
                        let chromatic = child_text(t, "chromatic")
                            .and_then(|v| v.parse::<i32>().ok())
                            .unwrap_or(0);
                        let octaves = child_text(t, "octave-change")
                            .and_then(|v| v.parse::<i32>().ok())
                            .unwrap_or(0);
                        transpose = chromatic.saturating_add(octaves.saturating_mul(12));
                    }
                }
                "backup" => position = (position - duration_of(element)).max(0),
                "forward" => position = position.saturating_add(duration_of(element)),
                "note" => {
                    let staff: u32 = child_text(element, "staff")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1);
                    if setup.tab_staff.is_some_and(|tab| tab != staff) {
                        continue;
                    }
                    let is_chord = child(element, "chord").is_some();
                    let length = duration_of(element);
                    let start = if is_chord { last_start } else { position };
                    if !is_chord {
                        last_start = start;
                        position = position.saturating_add(length);
                    }
                    if child(element, "grace").is_some() {
                        if child(element, "pitch").is_some() {
                            *skipped += 1;
                        }
                        continue;
                    }

                    let voice_key = (
                        staff,
                        child_text(element, "voice").unwrap_or_else(|| "1".into()),
                    );
                    let voice = match voice_keys.iter().position(|k| *k == voice_key) {
                        Some(i) => i,
                        None if voice_keys.len() < MAX_VOICES => {
                            voice_keys.push(voice_key);
                            voice_keys.len() - 1
                        }
                        None => {
                            if child(element, "rest").is_none() {
                                *skipped += 1;
                            }
                            continue;
                        }
                    };

                    let kind = if child(element, "rest").is_some() {
                        RawKind::Rest
                    } else if child(element, "unpitched").is_some() {
                        has_unpitched = true;
                        *skipped += 1;
                        RawKind::Rest
                    } else {
                        let technical =
                            child(element, "notations").and_then(|n| child(n, "technical"));
                        let fixed = technical.and_then(|t| {
                            let string = child_text(t, "string")?.parse::<u8>().ok()?;
                            let fret = child_text(t, "fret")?.parse::<u8>().ok()?;
                            (string >= 1 && fret <= MAX_FRET).then_some((string, fret))
                        });
                        let pitch = child(element, "pitch").and_then(|p| {
                            let step = child_text(p, "step")?;
                            let alter = child_text(p, "alter")
                                .and_then(|a| a.parse::<f64>().ok())
                                .map_or(0, |a| a.round() as i32);
                            let octave = child_text(p, "octave")?.parse::<i32>().ok()?;
                            let written = pitch_of(&step, alter, octave)?;
                            u8::try_from(written.checked_add(transpose)?)
                                .ok()
                                .filter(|p| *p <= 127)
                        });
                        has_pitched = true;
                        RawKind::Pitched { pitch, fixed }
                    };

                    let is_tie_end = element
                        .children()
                        .filter(|n| n.has_tag_name("tie"))
                        .chain(
                            child(element, "notations")
                                .into_iter()
                                .flat_map(|n| n.children().filter(|c| c.has_tag_name("tied"))),
                        )
                        .any(|t| t.attribute("type") == Some("stop"));
                    notes.push(RawNote {
                        voice,
                        start,
                        length,
                        kind,
                        is_tie_end,
                        technique: technique_of(element),
                    });
                }
                _ => {}
            }
        }

        raw_measures.push(RawMeasure {
            signature,
            is_implicit: measure.attribute("implicit") == Some("yes"),
            divisions,
            notes,
        });
    }

    // Percussion parts (only unpitched notes) have no strings to play on.
    if has_unpitched && !has_pitched {
        return Ok(None);
    }
    if raw_measures.is_empty() {
        return Ok(None);
    }

    let sound = sound_for(info);
    let instrument = sound.instrument();
    let tuning = setup
        .tuning
        .unwrap_or_else(|| instrument.standard_tuning().to_vec());
    let voice_count = voice_keys.len().max(1);

    let mut measures = Vec::with_capacity(raw_measures.len());
    for raw in &raw_measures {
        measures.push(build_measure(
            raw,
            voice_count,
            instrument,
            &tuning,
            skipped,
        )?);
    }

    let name = info
        .and_then(|i| i.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("Track {}", index + 1));
    Ok(Some(Track {
        name,
        instrument,
        sound,
        tuning,
        measures,
    }))
}

fn read_signature(time: Node) -> Option<TimeSignature> {
    // "beats" can be a sum such as "3+2".
    let numerator: u32 = child_text(time, "beats")?
        .split('+')
        .map(|part| part.trim().parse::<u32>().ok())
        .sum::<Option<u32>>()?;
    let denominator: u32 = child_text(time, "beat-type")?.trim().parse().ok()?;
    let signature = TimeSignature {
        numerator: u8::try_from(numerator).ok()?,
        denominator: u8::try_from(denominator).ok()?,
    };
    signature.is_valid().then_some(signature)
}

fn technique_of(note: Node) -> Option<Technique> {
    let notations = child(note, "notations")?;
    if let Some(technical) = child(notations, "technical") {
        for element in technical.children().filter(Node::is_element) {
            let ends = element.attribute("type") == Some("stop");
            match element.tag_name().name() {
                "hammer-on" if ends => return Some(Technique::HammerOn),
                "pull-off" if ends => return Some(Technique::PullOff),
                "harmonic" => return Some(Technique::Harmonic),
                "bend" => return Some(Technique::Bend),
                "other-technical" if element.text().map(str::trim) == Some("palm mute") => {
                    return Some(Technique::PalmMute)
                }
                _ => {}
            }
        }
    }
    notations
        .children()
        .filter(|n| n.has_tag_name("slide") || n.has_tag_name("glissando"))
        .any(|n| n.attribute("type") == Some("start"))
        .then_some(Technique::Slide)
}

/// Builds one measure: every voice quantized, placed on the strings and filled to the end.
fn build_measure(
    raw: &RawMeasure,
    voice_count: usize,
    instrument: Instrument,
    tuning: &[u8],
    skipped: &mut usize,
) -> Result<Measure, MusicXmlImportError> {
    let to_ticks = |divisions: i64| -> i64 {
        let ticks = divisions as f64 * TICKS_PER_QUARTER / raw.divisions;
        (ticks / GRID as f64).round() as i64 * GRID
    };

    let mut signature = raw.signature;
    if raw.is_implicit {
        let content = raw
            .notes
            .iter()
            .map(|n| to_ticks(n.start.saturating_add(n.length)))
            .max()
            .unwrap_or(0);
        if content > 0 && content < i64::from(signature.ticks()) {
            if let Some(short) = signature_for(content as u32) {
                signature = short;
            }
        }
    }
    let measure_ticks = i64::from(signature.ticks());

    let mut voices = Vec::with_capacity(voice_count);
    for voice in 0..voice_count {
        let mut events: Vec<&RawNote> = raw.notes.iter().filter(|n| n.voice == voice).collect();
        events.sort_by_key(|n| n.start);
        let strings = Strings { instrument, tuning };
        let beats = fill_voice(&events, measure_ticks, &to_ticks, strings, skipped)?;
        voices.push(Voice { beats });
    }
    Ok(Measure {
        time_signature: signature,
        voices,
    })
}

/// A time signature of exactly `ticks`, in the simplest denominator.
fn signature_for(ticks: u32) -> Option<TimeSignature> {
    [(32, 4), (16, 8), (8, 16), (4, 32)]
        .into_iter()
        .find(|&(unit, _)| ticks.is_multiple_of(unit))
        .and_then(|(unit, denominator)| {
            let signature = TimeSignature {
                numerator: u8::try_from(ticks / unit).ok()?,
                denominator,
            };
            signature.is_valid().then_some(signature)
        })
}

/// The strings of the part's instrument, to place notes on.
#[derive(Clone, Copy)]
struct Strings<'a> {
    instrument: Instrument,
    tuning: &'a [u8],
}

fn fill_voice(
    events: &[&RawNote],
    measure_ticks: i64,
    to_ticks: &dyn Fn(i64) -> i64,
    strings: Strings,
    skipped: &mut usize,
) -> Result<Vec<Beat>, MusicXmlImportError> {
    // Group the notes that start together (chords), on the grid.
    let mut groups: Vec<(i64, Vec<&RawNote>)> = Vec::new();
    for &event in events {
        let start = to_ticks(event.start).clamp(0, measure_ticks);
        match groups.last_mut() {
            Some((s, members)) if *s == start => members.push(event),
            _ => groups.push((start, vec![event])),
        }
    }

    let mut beats: Vec<Beat> = Vec::new();
    let mut cursor: i64 = 0;
    for (i, (start, members)) in groups.iter().enumerate() {
        let start = (*start).max(cursor);
        let next_start = groups
            .get(i + 1)
            .map_or(measure_ticks, |(s, _)| (*s).max(start));
        let end = members
            .iter()
            .map(|n| to_ticks(n.start.saturating_add(n.length)))
            .max()
            .unwrap_or(start)
            .min(next_start)
            .min(measure_ticks);

        let pitched = members
            .iter()
            .filter(|n| matches!(n.kind, RawKind::Pitched { .. }));
        if end <= start {
            // Squeezed out by the notes around it or by the end of the measure.
            *skipped += pitched.count();
            continue;
        }
        push_rests(&mut beats, start - cursor)?;
        let notes = place_notes(members, strings, skipped);
        push_beat(&mut beats, end - start, &notes)?;
        cursor = end;
    }
    push_rests(&mut beats, measure_ticks - cursor)?;
    Ok(beats)
}

fn place_notes(members: &[&RawNote], strings: Strings, skipped: &mut usize) -> Vec<Note> {
    let Strings { instrument, tuning } = strings;
    let mut notes: Vec<Note> = Vec::new();
    let mut loose: Vec<(u8, &RawNote)> = Vec::new();

    for &member in members {
        let RawKind::Pitched { pitch, fixed } = member.kind else {
            continue;
        };
        let note = |string: u8, fret: u8| Note {
            string,
            fret,
            technique: member.technique,
            tied: member.is_tie_end,
        };
        match (fixed, pitch) {
            // A fret the string does not have (the first frets of a banjo's short string) is
            // placed again from the pitch.
            (Some((string, fret)), _)
                if fret_pitch(instrument, tuning, string, fret).is_some()
                    && !notes.iter().any(|n| n.string == string) =>
            {
                notes.push(note(string, fret));
            }
            (_, Some(pitch)) => loose.push((pitch, member)),
            _ => *skipped += 1,
        }
    }

    let taken: Vec<u8> = notes.iter().map(|n| n.string).collect();
    let pitches: Vec<u8> = loose.iter().map(|&(p, _)| p).collect();
    let (placed, left_out) = place_chord(instrument, tuning, &pitches, &taken);
    *skipped += left_out;
    for mut note in placed {
        // Carry the tie and technique of the source note over to where it was placed.
        let pitch = fret_pitch(instrument, tuning, note.string, note.fret);
        if let Some((_, source)) = loose.iter().find(|(p, _)| Some(*p) == pitch) {
            note.tied = source.is_tie_end;
            note.technique = source.technique;
        }
        notes.push(note);
    }
    notes.sort_by_key(|n| n.string);
    notes
}

/// Appends rests adding up to `length` ticks (nothing for 0).
fn push_rests(beats: &mut Vec<Beat>, length: i64) -> Result<(), MusicXmlImportError> {
    if length > 0 {
        push_beat(beats, length, &[])?;
    }
    Ok(())
}

/// Appends a beat of `length` ticks, as several tied beats when no single duration fits.
fn push_beat(
    beats: &mut Vec<Beat>,
    length: i64,
    notes: &[Note],
) -> Result<(), MusicXmlImportError> {
    let durations: Vec<Duration> = u32::try_from(length)
        .ok()
        .and_then(decompose_ticks)
        .ok_or_else(|| MusicXmlImportError::Invalid(format!("a length of {length} ticks")))?;
    for (i, duration) in durations.into_iter().enumerate() {
        beats.push(Beat {
            duration,
            notes: notes
                .iter()
                .map(|n| Note {
                    tied: n.tied || i > 0,
                    ..n.clone()
                })
                .collect(),
        });
    }
    Ok(())
}

/// Parts of different lengths are padded with measures of rest, so that every track has the same
/// number of measures (the editor relies on it).
fn pad_to_same_length(tracks: &mut [Track]) -> Result<(), MusicXmlImportError> {
    let longest = tracks.iter().map(|t| t.measures.len()).max().unwrap_or(0);
    for track in tracks {
        let signature = track.measures.last().map_or(
            TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            |m| m.time_signature,
        );
        while track.measures.len() < longest {
            let mut beats = Vec::new();
            push_rests(&mut beats, i64::from(signature.ticks()))?;
            track.measures.push(Measure {
                time_signature: signature,
                voices: vec![Voice { beats }],
            });
        }
    }
    Ok(())
}

// ---- XML helpers ----------------------------------------------------------------------------

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|n| n.has_tag_name(name))
}

fn child_text(node: Node, name: &str) -> Option<String> {
    child(node, name)
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// The `duration` child as a whole number of divisions, 0 when missing or unreadable.
fn duration_of(node: Node) -> i64 {
    child_text(node, "duration")
        .and_then(|t| t.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d >= 0.0)
        .map_or(0, |d| d.min(1e9) as i64)
}

#[cfg(test)]
mod tests;
