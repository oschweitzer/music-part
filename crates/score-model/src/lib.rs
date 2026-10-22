//! Score model: Score > Track > Measure > Voice > Beat > Note.

mod chords;
mod demo;
mod edit;
mod fretting;
mod percussion;
mod sound;
mod validate;

pub use chords::{drum_segments, pitched_segments, Segment, Span};
pub use demo::{blank_score, demo_score};
pub use edit::{apply_edit, Edit, EditError};
pub use fretting::{place_chord, place_pitch};
pub use percussion::{
    drum_piece, drum_piece_at, is_played_by_foot, DrumPiece, NoteHead, DRUM_PIECES, PIECES,
};
pub use sound::{BuiltInSound, Sound, SoundIdError, BUILT_IN_SOUNDS, MAX_SOUND_ID_LEN};
pub use validate::ScoreError;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    pub tempo_bpm: u16,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "TrackFile")]
pub struct Track {
    pub name: String,
    pub instrument: Instrument,
    /// How the track sounds; always a sound of `instrument`: a built-in one, or one a sound pack
    /// declares (which may not be installed where the score is opened).
    pub sound: Sound,
    /// The General MIDI program that plays a pack's sound when its pack is not there, as the pack
    /// declared it. `None` for a built-in sound, which has its own program. Left out of files when
    /// `None`, so tracks with a built-in sound are written as before. See `Track::midi_program`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_program: Option<u8>,
    /// MIDI pitches of the open strings, from string 1 to the last. String 1 is the highest
    /// except on a banjo, whose short fifth string is tuned higher (see `Instrument::first_fret`).
    /// Empty for an instrument without strings (`NoteKind::Pitched`, `NoteKind::Percussion`).
    pub tuning: Vec<u8>,
    pub measures: Vec<Measure>,
    /// The clef of a track whose instrument lets the user choose it (`Instrument::has_clef_choice`:
    /// a voice), `None` for every other instrument, whose clefs follow from the instrument. Left
    /// out of files when `None`, so tracks of the other instruments are written as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clef: Option<Clef>,
}

/// A track as files store it. Files written before sounds existed have no `sound`: the track then
/// gets the default sound of its instrument, which is the sound it used to have. A voice without
/// a `clef` gets the default clef of its instrument.
#[derive(Deserialize)]
struct TrackFile {
    name: String,
    instrument: Instrument,
    #[serde(default)]
    sound: Option<Sound>,
    tuning: Vec<u8>,
    measures: Vec<Measure>,
    #[serde(default)]
    clef: Option<Clef>,
    #[serde(default)]
    fallback_program: Option<u8>,
}

impl From<TrackFile> for Track {
    fn from(file: TrackFile) -> Self {
        let sound = file
            .sound
            .unwrap_or_else(|| file.instrument.default_sound());
        Track {
            // A built-in sound has its own program: a fallback written next to one means nothing.
            fallback_program: file.fallback_program.filter(|_| !sound.is_built_in()),
            sound,
            name: file.name,
            instrument: file.instrument,
            tuning: file.tuning,
            measures: file.measures,
            clef: file.clef.or_else(|| file.instrument.default_clef()),
        }
    }
}

/// An instrument family: it decides how its notes are stored (`NoteKind`), the strings, how
/// frets are counted and the default tuning. How it sounds is the track's `Sound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Instrument {
    /// Acoustic guitar, with nylon or steel strings (the name predates the steel sound).
    Guitar,
    ElectricGuitar,
    Bass,
    Violin,
    /// Five-string banjo, with its short fifth string.
    Banjo,
    /// A keyboard: notes are pitches, written on a grand staff. See `docs/notation.md`.
    Piano,
    /// A drum kit: notes are percussion pieces, played on MIDI channel 10. See `docs/drums.md`.
    Drums,
    /// A keyboard written on a grand staff, like the piano (no pedal staff).
    Organ,
    /// A synthesizer, written on one treble staff.
    Synth,
    /// A singing voice, written on one staff in the clef of the track (`Track::clef`).
    Voice,
}

/// The clef of a track that has one staff and lets the user choose it (a voice: treble for
/// soprano and alto, treble an octave down for tenor, bass for baritone and bass). See
/// `docs/notation.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Clef {
    Treble,
    /// The treble clef with an 8 below: notes sound an octave lower than written.
    Treble8vb,
    Bass,
}

/// How the notes of an instrument are stored, and so how they are entered and written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// On a string and a fret (`Place::Fretted`), written as tablature and optionally a staff.
    Fretted,
    /// At a MIDI pitch (`Place::Pitched`), written on a staff. The track has no strings.
    Pitched,
    /// A piece of a drum kit (`Place::Percussion`), written on a percussion staff. The track has
    /// no strings.
    Percussion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub time_signature: TimeSignature,
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Voice {
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Beat {
    pub duration: Duration,
    /// Vide = silence.
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duration {
    pub value: NoteValue,
    pub dots: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Where the note is played. Files write its fields flat in the note, so a fretted note reads
    /// `{"string":1,"fret":3,...}` as it always did and a pitched one `{"pitch":60,...}`.
    #[serde(flatten)]
    pub place: Place,
    pub technique: Option<Technique>,
    /// Tie: this note continues the same pitch from the previous beat of the
    /// voice (across barlines too) without being replayed.
    #[serde(default)]
    pub tied: bool,
}

/// Where a note is played: a string and a fret, or a pitch. A track holds only the kind of its
/// instrument (`Instrument::note_kind`, checked by `Score::validate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Place {
    Fretted {
        /// String number, 1 = the highest.
        string: u8,
        fret: u8,
    },
    Pitched {
        /// MIDI pitch, 60 = middle C (C4).
        pitch: u8,
    },
    Percussion {
        /// General MIDI percussion key of the piece, 35 (acoustic kick) to 81 (open triangle).
        piece: u8,
    },
}

impl Place {
    pub fn kind(self) -> NoteKind {
        match self {
            Place::Fretted { .. } => NoteKind::Fretted,
            Place::Pitched { .. } => NoteKind::Pitched,
            Place::Percussion { .. } => NoteKind::Percussion,
        }
    }
}

impl Note {
    /// A note on a string and fret, untied, without technique.
    pub fn fretted(string: u8, fret: u8) -> Note {
        Note {
            place: Place::Fretted { string, fret },
            technique: None,
            tied: false,
        }
    }

    /// A note at a MIDI pitch, untied, without technique.
    pub fn pitched(pitch: u8) -> Note {
        Note {
            place: Place::Pitched { pitch },
            technique: None,
            tied: false,
        }
    }

    /// A hit on a piece of a drum kit (its General MIDI percussion key), untied.
    pub fn percussion(piece: u8) -> Note {
        Note {
            place: Place::Percussion { piece },
            technique: None,
            tied: false,
        }
    }

    /// The string of a fretted note.
    pub fn string(&self) -> Option<u8> {
        match self.place {
            Place::Fretted { string, .. } => Some(string),
            Place::Pitched { .. } | Place::Percussion { .. } => None,
        }
    }

    /// The fret of a fretted note.
    pub fn fret(&self) -> Option<u8> {
        match self.place {
            Place::Fretted { fret, .. } => Some(fret),
            Place::Pitched { .. } | Place::Percussion { .. } => None,
        }
    }

    /// What tells the note apart from the others of its beat: its string, its pitch, or its drum
    /// piece. Beats keep their notes sorted by it, with no two notes sharing one.
    pub fn key(&self) -> u8 {
        match self.place {
            Place::Fretted { string, .. } => string,
            Place::Pitched { pitch } => pitch,
            Place::Percussion { piece } => piece,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    HammerOn,
    PullOff,
    Slide,
    /// A bend of a whole tone ("full"). It is the bend files wrote before amounts existed, so its
    /// name stays; the smaller bends are `BendHalf` and `BendQuarter`.
    Bend,
    /// A bend of a half tone (a semitone).
    BendHalf,
    /// A bend of a quarter tone (half a semitone).
    BendQuarter,
    PalmMute,
    Harmonic,
}

impl Technique {
    /// True for the three bends, whatever their amount.
    pub fn is_bend(self) -> bool {
        self.bend_semitones().is_some()
    }

    /// How far a bend rises above the note, in semitones; `None` for the other techniques.
    pub fn bend_semitones(self) -> Option<f32> {
        match self {
            Technique::Bend => Some(2.0),
            Technique::BendHalf => Some(1.0),
            Technique::BendQuarter => Some(0.5),
            Technique::HammerOn
            | Technique::PullOff
            | Technique::Slide
            | Technique::PalmMute
            | Technique::Harmonic => None,
        }
    }
}

impl NoteValue {
    /// Duration in 128th notes (whole note = 128).
    fn ticks(self) -> u32 {
        match self {
            NoteValue::Whole => 128,
            NoteValue::Half => 64,
            NoteValue::Quarter => 32,
            NoteValue::Eighth => 16,
            NoteValue::Sixteenth => 8,
            NoteValue::ThirtySecond => 4,
        }
    }
}

/// Highest fret a note can be placed on.
pub const MAX_FRET: u8 = 24;

impl Duration {
    /// At most two dots, and none on a 32nd note (its dotted length is not a multiple of a 32nd).
    pub fn is_valid(self) -> bool {
        self.dots <= 2 && (self.value != NoteValue::ThirtySecond || self.dots == 0)
    }

    pub fn ticks(self) -> u32 {
        let base = self.value.ticks();
        let mut total = base;
        let mut add = base;
        for _ in 0..self.dots {
            add /= 2;
            total += add;
        }
        total
    }
}

impl TimeSignature {
    /// A power-of-two denominator up to 32, and a measure that is a whole number of 32nd notes.
    pub fn is_valid(self) -> bool {
        let ticks = self.ticks();
        self.numerator != 0
            && self.denominator.is_power_of_two()
            && self.denominator <= 32
            && ticks >= 4
            && ticks.is_multiple_of(4)
    }

    /// Length of a measure in 128th notes. A denominator of 0 counts as an empty measure.
    pub fn ticks(self) -> u32 {
        (self.numerator as u32 * 128)
            .checked_div(self.denominator as u32)
            .unwrap_or(0)
    }
}

/// Splits a length into valid durations, taking the largest that fits each time. `None` if the length is not a
/// multiple of a 32nd note, which no combination of valid durations can add up to.
pub fn decompose_ticks(mut len: u32) -> Option<Vec<Duration>> {
    const VALUES: [NoteValue; 6] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ];
    let mut candidates: Vec<Duration> = VALUES
        .iter()
        .flat_map(|&value| (0..=2).map(move |dots| Duration { value, dots }))
        .filter(|d| d.is_valid())
        .collect();
    candidates.sort_by_key(|d| std::cmp::Reverse(d.ticks()));

    let mut out = Vec::new();
    while len > 0 {
        let d = *candidates.iter().find(|d| d.ticks() <= len)?;
        len -= d.ticks();
        out.push(d);
    }
    Some(out)
}

impl Track {
    /// The General MIDI program that plays the track when no sound pack does: the program of a
    /// built-in sound, else the fallback the pack declared, else (a file that names none) the
    /// program of the default sound of the instrument. For drums it is the kit.
    pub fn midi_program(&self) -> u8 {
        self.sound
            .midi_program()
            .or(self.fallback_program)
            .or(self.instrument.default_sound().midi_program())
            .unwrap_or(0)
    }

    /// MIDI pitch of a note, or `None` if the string or the fret does not exist, or the pitch is
    /// above 127. A drum note gives the key of its piece, which is what the percussion channel
    /// plays, and `None` for a key that is no piece.
    pub fn pitch(&self, note: &Note) -> Option<u8> {
        match note.place {
            Place::Fretted { string, fret } => {
                fret_pitch(self.instrument, &self.tuning, string, fret)
            }
            Place::Pitched { pitch } => (pitch <= MAX_PITCH).then_some(pitch),
            Place::Percussion { piece } => DRUM_PIECES.contains(&piece).then_some(piece),
        }
    }
}

/// Highest MIDI pitch.
pub const MAX_PITCH: u8 = 127;

/// Highest General MIDI program (they are counted from 0).
pub const MAX_PROGRAM: u8 = 127;

/// MIDI pitch of a fret of a string (1 is the first) of an instrument, or `None` if the string or
/// the fret does not exist.
pub fn fret_pitch(instrument: Instrument, tuning: &[u8], string: u8, fret: u8) -> Option<u8> {
    let open = *tuning.get(usize::from(string.checked_sub(1)?))?;
    if fret == 0 {
        return Some(open);
    }
    let first = instrument.first_fret(string, tuning.len());
    let above_open = fret.checked_sub(first).filter(|s| *s > 0)?;
    open.checked_add(above_open).filter(|p| *p <= 127)
}

/// The fret that plays a MIDI pitch on a string of an instrument, or `None` if the string cannot
/// play it within `MAX_FRET`. The inverse of `fret_pitch`.
pub fn pitch_fret(instrument: Instrument, tuning: &[u8], string: u8, pitch: u8) -> Option<u8> {
    let open = *tuning.get(usize::from(string.checked_sub(1)?))?;
    let above_open = pitch.checked_sub(open)?;
    if above_open == 0 {
        return Some(0);
    }
    above_open
        .checked_add(instrument.first_fret(string, tuning.len()))
        .filter(|f| *f <= MAX_FRET)
}

impl Voice {
    pub fn ticks(&self) -> u32 {
        self.beats.iter().map(|b| b.duration.ticks()).sum()
    }
}

impl Measure {
    /// True if every voice fills the measure exactly.
    pub fn is_complete(&self) -> bool {
        let expected = self.time_signature.ticks();
        self.voices.iter().all(|v| v.ticks() == expected)
    }
}

/// Standard tuning of a 6-string guitar (E4 B3 G3 D3 A2 E2).
pub const GUITAR_STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

/// Standard tuning of a 4-string bass (G2 D2 A1 E1).
pub const BASS_STANDARD: [u8; 4] = [43, 38, 33, 28];

/// Standard tuning of a violin (E5 A4 D4 G3).
pub const VIOLIN_STANDARD: [u8; 4] = [76, 69, 62, 55];

/// Open G tuning of a five-string banjo, string 1 first: D4 B3 G3 D3, then the short fifth
/// string, G4.
pub const BANJO_STANDARD: [u8; 5] = [62, 59, 55, 50, 67];

/// The fret where the short fifth string of a banjo starts.
const BANJO_FIFTH_STRING_START: u8 = 5;

impl Instrument {
    pub const ALL: [Instrument; 10] = [
        Instrument::Guitar,
        Instrument::ElectricGuitar,
        Instrument::Bass,
        Instrument::Violin,
        Instrument::Banjo,
        Instrument::Piano,
        Instrument::Organ,
        Instrument::Synth,
        Instrument::Voice,
        Instrument::Drums,
    ];

    /// How the notes of a track of this instrument are stored.
    pub fn note_kind(self) -> NoteKind {
        match self {
            Instrument::Guitar
            | Instrument::ElectricGuitar
            | Instrument::Bass
            | Instrument::Violin
            | Instrument::Banjo => NoteKind::Fretted,
            Instrument::Piano | Instrument::Organ | Instrument::Synth | Instrument::Voice => {
                NoteKind::Pitched
            }
            Instrument::Drums => NoteKind::Percussion,
        }
    }

    /// Open-string pitches, string 1 first. Empty for an instrument without strings.
    pub fn standard_tuning(self) -> &'static [u8] {
        match self {
            Instrument::Guitar | Instrument::ElectricGuitar => &GUITAR_STANDARD,
            Instrument::Bass => &BASS_STANDARD,
            Instrument::Violin => &VIOLIN_STANDARD,
            Instrument::Banjo => &BANJO_STANDARD,
            Instrument::Piano
            | Instrument::Organ
            | Instrument::Synth
            | Instrument::Voice
            | Instrument::Drums => &[],
        }
    }

    /// The fret where a string starts, 0 for a string that runs the whole neck. The fifth string
    /// of a five-string banjo starts at fret 5: tablature numbers its frets like the other
    /// strings, so it has no frets 1 to 5 and its fret 6 is one semitone above the open string.
    pub fn first_fret(self, string: u8, string_count: usize) -> u8 {
        match self {
            Instrument::Banjo if string == 5 && string_count == 5 => BANJO_FIFTH_STRING_START,
            _ => 0,
        }
    }

    /// The instrument for a General MIDI program, when a file names one. A drum kit is not chosen
    /// by a program but by the percussion channel (MIDI channel 10), so this is never `Drums`.
    pub fn from_midi_program(program: Option<u8>) -> Instrument {
        Sound::from_midi_program(program).instrument()
    }

    /// True for an instrument written on one staff whose clef the track chooses (`Track::clef`):
    /// a voice, whose clef depends on the singer's range.
    pub fn has_clef_choice(self) -> bool {
        self == Instrument::Voice
    }

    /// The clef of a new track of this instrument: the treble clef for a voice, `None` for an
    /// instrument without a clef choice.
    pub fn default_clef(self) -> Option<Clef> {
        self.has_clef_choice().then_some(Clef::Treble)
    }
}

/// Mean pitch from which a part is written in the treble clef (B3).
const TREBLE_FROM: u64 = 59;
/// Mean pitch from which a part below `TREBLE_FROM` is written in the treble clef an octave down
/// (F3); lower parts take the bass clef.
const TREBLE_8VB_FROM: u64 = 53;

impl Clef {
    pub const ALL: [Clef; 3] = [Clef::Treble, Clef::Treble8vb, Clef::Bass];

    /// The clef that suits a part with these pitches, by their mean: the treble clef for soprano
    /// and alto parts (mean from B3), an octave down for tenor (from F3), bass below. No pitch
    /// gives the treble clef.
    pub fn for_pitches(pitches: impl IntoIterator<Item = u8>) -> Clef {
        let (sum, count) = pitches.into_iter().fold((0u64, 0u64), |(sum, count), p| {
            (sum + u64::from(p), count + 1)
        });
        match sum.checked_div(count) {
            None => Clef::Treble,
            Some(mean) if mean >= TREBLE_FROM => Clef::Treble,
            Some(mean) if mean >= TREBLE_8VB_FROM => Clef::Treble8vb,
            Some(_) => Clef::Bass,
        }
    }
}

impl Track {
    /// The clef that suits the notes of this track when its instrument has a clef choice (see
    /// `Clef::for_pitches`), else `None`. Importers and a change of instrument use it.
    pub fn fitting_clef(&self) -> Option<Clef> {
        if !self.instrument.has_clef_choice() {
            return None;
        }
        let pitches = self
            .measures
            .iter()
            .flat_map(|m| &m.voices)
            .flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes)
            .filter_map(|n| self.pitch(n));
        Some(Clef::for_pitches(pitches))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarter() -> Duration {
        Duration {
            value: NoteValue::Quarter,
            dots: 0,
        }
    }

    fn guitar() -> Track {
        Track {
            name: "Guitar".into(),
            instrument: Instrument::Guitar,
            sound: Sound::GUITAR_NYLON,
            fallback_program: None,
            tuning: GUITAR_STANDARD.to_vec(),
            measures: vec![],
            clef: None,
        }
    }

    #[test]
    fn standard_tunings_are_listed_highest_full_length_string_first() {
        for instrument in Instrument::ALL {
            if instrument.note_kind() != NoteKind::Fretted {
                continue;
            }
            let tuning = instrument.standard_tuning();
            let full_length: Vec<u8> = (1..=tuning.len() as u8)
                .filter(|&s| instrument.first_fret(s, tuning.len()) == 0)
                .map(|s| tuning[usize::from(s) - 1])
                .collect();
            assert!(
                full_length.windows(2).all(|w| w[0] > w[1]),
                "{instrument:?}"
            );
        }
        assert_eq!(Instrument::Bass.standard_tuning().len(), 4);
    }

    #[test]
    fn the_program_of_a_file_picks_the_instrument_of_its_sound() {
        assert_eq!(Instrument::from_midi_program(None), Instrument::Guitar);
        assert_eq!(Instrument::from_midi_program(Some(25)), Instrument::Guitar);
        assert_eq!(
            Instrument::from_midi_program(Some(30)),
            Instrument::ElectricGuitar
        );
        assert_eq!(Instrument::from_midi_program(Some(105)), Instrument::Banjo);
        assert_eq!(Instrument::from_midi_program(Some(0)), Instrument::Piano);
        assert_eq!(Instrument::from_midi_program(Some(73)), Instrument::Piano);
    }

    fn banjo() -> Track {
        Track {
            name: "Banjo".into(),
            instrument: Instrument::Banjo,
            sound: Sound::BANJO,
            fallback_program: None,
            tuning: BANJO_STANDARD.to_vec(),
            measures: vec![],
            clef: None,
        }
    }

    fn at(string: u8, fret: u8) -> Note {
        Note::fretted(string, fret)
    }

    #[test]
    fn the_short_fifth_banjo_string_counts_frets_from_the_neck() {
        let t = banjo();
        assert_eq!(t.pitch(&at(5, 0)), Some(67));
        for fret in 1..=5 {
            assert_eq!(t.pitch(&at(5, fret)), None, "fret {fret}");
        }
        assert_eq!(t.pitch(&at(5, 6)), Some(68));
        assert_eq!(t.pitch(&at(5, 7)), Some(69));
        // The long strings are fretted as usual.
        assert_eq!(t.pitch(&at(4, 2)), Some(52));
    }

    #[test]
    fn finding_the_fret_of_a_pitch_is_the_inverse_of_its_pitch() {
        for string in 1..=5 {
            for pitch in 40..=100 {
                if let Some(fret) = pitch_fret(Instrument::Banjo, &BANJO_STANDARD, string, pitch) {
                    assert_eq!(
                        fret_pitch(Instrument::Banjo, &BANJO_STANDARD, string, fret),
                        Some(pitch)
                    );
                }
            }
        }
        assert_eq!(
            pitch_fret(Instrument::Banjo, &BANJO_STANDARD, 5, 69),
            Some(7)
        );
        assert_eq!(
            pitch_fret(Instrument::Guitar, &GUITAR_STANDARD, 1, 69),
            Some(5)
        );
    }

    #[test]
    fn a_track_without_a_sound_gets_the_default_sound_of_its_instrument() {
        let json = r#"{"name":"B","instrument":"Bass","tuning":[43,38,33,28],"measures":[]}"#;
        let track: Track = serde_json::from_str(json).unwrap();
        assert_eq!(track.sound, Sound::BASS_FINGER);
    }

    #[test]
    fn the_sound_of_a_track_is_saved_and_read_back() {
        let mut track = guitar();
        track.instrument = Instrument::ElectricGuitar;
        track.sound = Sound::ELECTRIC_GUITAR_DISTORTION;
        let json = serde_json::to_string(&track).unwrap();
        assert!(
            json.contains(r#""sound":"electric-guitar/distortion""#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Track>(&json).unwrap(), track);
    }

    #[test]
    fn a_track_with_a_built_in_sound_is_written_without_a_fallback_program() {
        let json = serde_json::to_string(&guitar()).unwrap();
        assert!(!json.contains("fallback_program"), "{json}");
    }

    #[test]
    fn a_pack_sound_and_its_fallback_program_are_saved_and_read_back() {
        let mut track = guitar();
        track.sound = Sound::parse("guitar/twelve-string").unwrap();
        track.fallback_program = Some(25);
        let json = serde_json::to_string(&track).unwrap();
        assert!(
            json.contains(r#""sound":"guitar/twelve-string","fallback_program":25"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Track>(&json).unwrap(), track);
    }

    #[test]
    fn a_pack_sound_plays_its_fallback_program_or_the_default_one_of_its_instrument() {
        let mut track = guitar();
        assert_eq!(track.midi_program(), 24);
        track.instrument = Instrument::ElectricGuitar;
        track.sound = Sound::parse("electric-guitar/fuzz").unwrap();
        assert_eq!(track.midi_program(), 27, "the clean electric guitar");
        track.fallback_program = Some(30);
        assert_eq!(track.midi_program(), 30);
    }

    #[test]
    fn a_fallback_program_next_to_a_built_in_sound_is_dropped_on_reading() {
        let json = r#"{"name":"G","instrument":"Guitar","sound":"guitar/steel","fallback_program":3,
            "tuning":[64,59,55,50,45,40],"measures":[]}"#;
        let track: Track = serde_json::from_str(json).unwrap();
        assert_eq!(
            (track.sound, track.fallback_program),
            (Sound::GUITAR_STEEL, None)
        );
    }

    #[test]
    fn decompose_uses_the_largest_durations_first() {
        let d = |value, dots| Duration { value, dots };
        assert_eq!(decompose_ticks(128), Some(vec![d(NoteValue::Whole, 0)]));
        assert_eq!(decompose_ticks(48), Some(vec![d(NoteValue::Quarter, 1)]));
        assert_eq!(
            decompose_ticks(36),
            Some(vec![
                d(NoteValue::Quarter, 0),
                d(NoteValue::ThirtySecond, 0)
            ])
        );
        assert_eq!(decompose_ticks(0), Some(vec![]));
    }

    #[test]
    fn decompose_always_adds_back_up_and_refuses_what_is_not_a_32nd_multiple() {
        for len in (0..=512).step_by(4) {
            let parts = decompose_ticks(len).unwrap();
            assert_eq!(parts.iter().map(|d| d.ticks()).sum::<u32>(), len);
            assert!(parts.iter().all(|d| d.is_valid()));
        }
        assert_eq!(decompose_ticks(6), None);
        assert_eq!(decompose_ticks(3), None);
    }

    #[test]
    fn a_dotted_32nd_is_not_a_valid_duration() {
        assert!(!Duration {
            value: NoteValue::ThirtySecond,
            dots: 1
        }
        .is_valid());
        assert!(!Duration {
            value: NoteValue::Quarter,
            dots: 3
        }
        .is_valid());
        assert!(Duration {
            value: NoteValue::Sixteenth,
            dots: 2
        }
        .is_valid());
    }

    #[test]
    fn a_zero_denominator_does_not_divide_by_zero() {
        assert_eq!(
            TimeSignature {
                numerator: 4,
                denominator: 0
            }
            .ticks(),
            0
        );
    }

    #[test]
    fn dotted_duration() {
        let d = Duration {
            value: NoteValue::Quarter,
            dots: 1,
        };
        assert_eq!(d.ticks(), 48);
    }

    #[test]
    fn pitch_from_string_and_fret() {
        let t = guitar();
        let open_e = Note::fretted(6, 0);
        let a_string_5th = Note::fretted(5, 5);
        assert_eq!(t.pitch(&open_e), Some(40));
        assert_eq!(t.pitch(&a_string_5th), Some(50));
        assert_eq!(t.pitch(&Note::fretted(7, 0)), None);
        assert_eq!(t.pitch(&Note::fretted(0, 0)), None);
    }

    #[test]
    fn measure_completeness() {
        let beat = Beat {
            duration: quarter(),
            notes: vec![],
        };
        let mut m = Measure {
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            voices: vec![Voice {
                beats: vec![beat.clone(); 4],
            }],
        };
        assert!(m.is_complete());
        m.voices[0].beats.pop();
        assert!(!m.is_complete());
    }

    #[test]
    fn note_without_tied_field_still_deserializes() {
        let note: Note = serde_json::from_str(r#"{"string":1,"fret":3,"technique":null}"#).unwrap();
        assert!(!note.tied);
        assert_eq!(note.place, Place::Fretted { string: 1, fret: 3 });
    }

    #[test]
    fn a_fretted_note_is_written_as_it_always_was() {
        let json = serde_json::to_string(&Note::fretted(2, 5)).unwrap();
        assert_eq!(
            json,
            r#"{"string":2,"fret":5,"technique":null,"tied":false}"#
        );
    }

    #[test]
    fn a_pitched_note_is_written_with_its_pitch_and_read_back() {
        let mut note = Note::pitched(61);
        note.tied = true;
        let json = serde_json::to_string(&note).unwrap();
        assert_eq!(json, r#"{"pitch":61,"technique":null,"tied":true}"#);
        assert_eq!(serde_json::from_str::<Note>(&json).unwrap(), note);
    }

    #[test]
    fn a_note_with_neither_a_string_nor_a_pitch_is_refused() {
        assert!(serde_json::from_str::<Note>(r#"{"fret":3,"technique":null}"#).is_err());
    }

    #[test]
    fn a_pitched_note_sounds_at_its_pitch_on_any_track() {
        let piano = Track {
            name: "Piano".into(),
            instrument: Instrument::Piano,
            sound: Sound::PIANO,
            fallback_program: None,
            tuning: vec![],
            measures: vec![],
            clef: None,
        };
        assert_eq!(piano.pitch(&Note::pitched(60)), Some(60));
        assert_eq!(piano.pitch(&Note::pitched(128)), None);
        assert_eq!(Note::pitched(60).key(), 60);
        assert_eq!(Note::fretted(3, 5).key(), 3);
        assert_eq!(Note::pitched(60).string(), None);
    }

    #[test]
    fn a_drum_note_is_written_with_its_piece_and_read_back() {
        let note = Note::percussion(42);
        let json = serde_json::to_string(&note).unwrap();
        assert_eq!(json, r#"{"piece":42,"technique":null,"tied":false}"#);
        assert_eq!(serde_json::from_str::<Note>(&json).unwrap(), note);
        assert_eq!(note.key(), 42);
        assert_eq!((note.string(), note.fret()), (None, None));
    }

    #[test]
    fn a_drum_note_sounds_at_the_key_of_its_piece() {
        let drums = Track {
            name: "Drums".into(),
            instrument: Instrument::Drums,
            sound: Sound::DRUMS_STANDARD,
            fallback_program: None,
            tuning: vec![],
            measures: vec![],
            clef: None,
        };
        assert_eq!(drums.pitch(&Note::percussion(36)), Some(36));
        assert_eq!(drums.pitch(&Note::percussion(34)), None);
        assert_eq!(drums.pitch(&Note::percussion(82)), None);
        assert_eq!(Instrument::Drums.note_kind(), NoteKind::Percussion);
    }

    #[test]
    fn only_instruments_without_strings_store_pitches() {
        for instrument in Instrument::ALL {
            let has_strings = !instrument.standard_tuning().is_empty();
            // Drums have no strings either: they store pieces.
            if instrument.note_kind() == NoteKind::Percussion {
                assert!(!has_strings);
                continue;
            }
            assert_eq!(
                instrument.note_kind() == NoteKind::Fretted,
                has_strings,
                "{instrument:?}"
            );
        }
        assert_eq!(Instrument::Piano.note_kind(), NoteKind::Pitched);
    }

    #[test]
    fn the_clef_of_a_part_follows_its_mean_pitch() {
        // Soprano, alto, tenor, baritone and bass lines.
        assert_eq!(Clef::for_pitches([67, 72, 74, 69]), Clef::Treble);
        assert_eq!(Clef::for_pitches([57, 62, 64, 60]), Clef::Treble);
        assert_eq!(Clef::for_pitches([55, 60, 57, 52]), Clef::Treble8vb);
        assert_eq!(Clef::for_pitches([45, 52, 55, 50]), Clef::Bass);
        assert_eq!(Clef::for_pitches([40, 43, 48, 45]), Clef::Bass);
        assert_eq!(Clef::for_pitches([]), Clef::Treble);
    }

    #[test]
    fn a_clef_is_written_in_camel_case() {
        let json = serde_json::to_string(&Clef::ALL).unwrap();
        assert_eq!(json, r#"["treble","treble8vb","bass"]"#);
    }

    #[test]
    fn json_roundtrip() {
        let score = Score {
            title: "Test".into(),
            tempo_bpm: 120,
            tracks: vec![guitar()],
        };
        let json = serde_json::to_string(&score).unwrap();
        let back: Score = serde_json::from_str(&json).unwrap();
        assert_eq!(score, back);
    }

    #[test]
    fn a_bend_written_before_amounts_existed_is_a_whole_tone() {
        let note: Note =
            serde_json::from_str(r#"{"string":1,"fret":3,"technique":"Bend"}"#).unwrap();
        assert_eq!(note.technique, Some(Technique::Bend));
        assert_eq!(Technique::Bend.bend_semitones(), Some(2.0));
    }

    #[test]
    fn bends_of_three_amounts_round_trip_and_the_other_techniques_are_no_bend() {
        for (technique, name, semitones) in [
            (Technique::BendQuarter, "BendQuarter", 0.5),
            (Technique::BendHalf, "BendHalf", 1.0),
            (Technique::Bend, "Bend", 2.0),
        ] {
            let json = serde_json::to_string(&technique).unwrap();
            assert_eq!(json, format!("\"{name}\""));
            assert_eq!(serde_json::from_str::<Technique>(&json).unwrap(), technique);
            assert!(technique.is_bend());
            assert_eq!(technique.bend_semitones(), Some(semitones));
        }
        for technique in [
            Technique::HammerOn,
            Technique::PullOff,
            Technique::Slide,
            Technique::PalmMute,
            Technique::Harmonic,
        ] {
            assert!(!technique.is_bend());
            assert_eq!(technique.bend_semitones(), None);
        }
    }
}
