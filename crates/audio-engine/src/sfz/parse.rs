//! Reads the text of an SFZ file into regions, every inherited opcode resolved. Pure: no file is
//! opened here. What is not supported is counted, never an error; see `docs/sfz.md`.

use std::collections::BTreeMap;

/// Most regions an SFZ file may declare.
pub const MAX_REGIONS: usize = 8192;
/// Most bytes of text once `#define`s are expanded.
pub const MAX_EXPANDED_BYTES: usize = 4 * 1024 * 1024;
/// Most `#define`s kept.
const MAX_DEFINES: usize = 256;
/// Most distinct names kept in the count of what was ignored; the rest go under one entry.
const MAX_IGNORED_NAMES: usize = 64;
/// The entry the ignored names past `MAX_IGNORED_NAMES` are counted under.
const OTHER_IGNORED: &str = "(other)";

/// How a region's sample loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// Plays once, stops at the note-off (after the release).
    NoLoop,
    /// Plays the whole sample once, ignoring the note-off.
    OneShot,
    /// Loops while the note sounds, release included.
    Continuous,
    /// Loops until the note-off, then plays on to the end of the sample.
    Sustain,
}

/// The amplitude envelope of a region (`ampeg_*`). Times in seconds, `sustain` in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            delay: 0.0,
            attack: 0.0,
            hold: 0.0,
            decay: 0.0,
            sustain: 100.0,
            release: 0.0,
        }
    }
}

/// One region with what it inherits from its `<group>`, `<master>` and `<global>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// The sample path as written, `default_path` prefixed, separators not yet checked.
    pub sample: String,
    pub lokey: u8,
    pub hikey: u8,
    pub lovel: u8,
    pub hivel: u8,
    /// Random range, the region plays when a note's random number in `[0, 1)` falls in it.
    pub lorand: f32,
    pub hirand: f32,
    /// Round-robin: the region plays on the `seq_position`-th of every `seq_length` matching
    /// notes, counted from 1.
    pub seq_length: u32,
    pub seq_position: u32,
    pub pitch_keycenter: u8,
    /// Cents per key.
    pub pitch_keytrack: f32,
    /// Semitones.
    pub transpose: i32,
    /// Cents.
    pub tune: f32,
    /// Decibels.
    pub volume: f32,
    /// Percent.
    pub amplitude: f32,
    /// -100 (left) to 100 (right).
    pub pan: f32,
    /// Percent, -100 to 100: how much the velocity changes the gain.
    pub amp_veltrack: f32,
    pub envelope: Envelope,
    /// First frame played.
    pub offset: u64,
    /// Last frame played, the end of the sample when `None`.
    pub end: Option<u64>,
    /// `None`: loops when the sample file declares a loop, else not.
    pub loop_mode: Option<LoopMode>,
    /// Loop points in frames, `loop_end` included; from the sample file when `None`.
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
    /// Played when a note starts (`trigger=attack`, the default). Other regions are left out.
    pub on_attack: bool,
}

impl Default for Region {
    fn default() -> Self {
        Self {
            sample: String::new(),
            lokey: 0,
            hikey: 127,
            lovel: 1,
            hivel: 127,
            lorand: 0.0,
            hirand: 1.0,
            seq_length: 1,
            seq_position: 1,
            pitch_keycenter: 60,
            pitch_keytrack: 100.0,
            transpose: 0,
            tune: 0.0,
            volume: 0.0,
            amplitude: 100.0,
            pan: 0.0,
            amp_veltrack: 100.0,
            envelope: Envelope::default(),
            offset: 0,
            end: None,
            loop_mode: None,
            loop_start: None,
            loop_end: None,
            on_attack: true,
        }
    }
}

/// What an SFZ file declares, and what was left out of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    pub regions: Vec<Region>,
    /// What was ignored, by name (an opcode, a header, a preprocessor directive), with how many
    /// times it was met.
    pub ignored: BTreeMap<String, u32>,
}

impl Parsed {
    fn ignore(&mut self, name: &str) {
        let known = self.ignored.contains_key(name);
        let key = if known || self.ignored.len() < MAX_IGNORED_NAMES {
            // Names come from the file: keep them short.
            name.chars().take(48).collect()
        } else {
            OTHER_IGNORED.to_string()
        };
        *self.ignored.entry(key).or_insert(0) += 1;
    }
}

/// Parses the text of an SFZ file. Fails only when a limit is passed (too many regions, a text
/// too large once expanded); anything else unknown or invalid is counted in `Parsed::ignored`.
pub fn parse(text: &str) -> Result<Parsed, String> {
    let text = strip_comments(text);
    let mut parsed = Parsed::default();
    let mut defines: Vec<(String, String)> = Vec::new();
    let mut expanded_bytes = 0usize;

    let mut header = Header::None;
    let mut default_path = String::new();
    // The opcodes of each level in force, in the order they were read.
    let mut global: Vec<(Opcode, String)> = Vec::new();
    let mut master: Vec<(Opcode, String)> = Vec::new();
    let mut group: Vec<(Opcode, String)> = Vec::new();
    let mut region: Vec<(Opcode, String)> = Vec::new();

    for raw_line in text.lines() {
        let trimmed = raw_line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("#define") {
            let mut words = rest.split_whitespace();
            match (words.next(), words.next()) {
                (Some(name), Some(value)) if name.starts_with('$') && name.len() > 1 => {
                    defines.retain(|(n, _)| n != name);
                    if defines.len() < MAX_DEFINES {
                        defines.push((name.to_string(), value.to_string()));
                    } else {
                        parsed.ignore("#define");
                    }
                }
                _ => parsed.ignore("#define"),
            }
            continue;
        }
        if trimmed.starts_with('#') {
            let directive = trimmed.split_whitespace().next().unwrap_or("#");
            parsed.ignore(directive);
            continue;
        }
        let line = expand(raw_line, &defines);
        expanded_bytes += line.len();
        if expanded_bytes > MAX_EXPANDED_BYTES {
            return Err("the SFZ text is too large".into());
        }

        for token in tokens(&line) {
            match token {
                Token::Header(name) => {
                    if header == Header::Region {
                        finish_region(
                            &mut parsed,
                            [&global, &master, &group, &region],
                            &default_path,
                        )?;
                    }
                    region.clear();
                    header = Header::from_name(name);
                    match header {
                        Header::Global => {
                            global.clear();
                            master.clear();
                            group.clear();
                        }
                        Header::Master => {
                            master.clear();
                            group.clear();
                        }
                        Header::Group => group.clear(),
                        Header::Other => parsed.ignore(&format!("<{name}>")),
                        Header::None | Header::Control | Header::Region => {}
                    }
                }
                Token::Opcode(name, value) => {
                    let level = match header {
                        Header::Global => &mut global,
                        Header::Master => &mut master,
                        Header::Group => &mut group,
                        Header::Region => &mut region,
                        Header::Control => {
                            if name == "default_path" {
                                default_path = value.replace('\\', "/");
                            } else {
                                parsed.ignore(name);
                            }
                            continue;
                        }
                        // The opcodes of an ignored header are counted with it.
                        Header::Other => continue,
                        Header::None => {
                            parsed.ignore(name);
                            continue;
                        }
                    };
                    match Opcode::from_name(name) {
                        Some(opcode) if opcode.is_valid(value) => {
                            level.push((opcode, value.to_string()))
                        }
                        Some(_) => parsed.ignore(&format!("{name}={value}")),
                        None => parsed.ignore(name),
                    }
                }
                Token::Junk => parsed.ignore("(unreadable text)"),
            }
        }
    }
    if header == Header::Region {
        finish_region(
            &mut parsed,
            [&global, &master, &group, &region],
            &default_path,
        )?;
    }
    Ok(parsed)
}

/// Reads a key as a MIDI number (`60`) or a note name (`c4`, `c#4`, `db4`, `c-1`), middle C
/// being `c4`.
pub fn parse_key(value: &str) -> Option<u8> {
    if let Ok(number) = value.parse::<i32>() {
        return u8::try_from(number).ok().filter(|k| *k <= 127);
    }
    let lower = value.to_ascii_lowercase();
    let mut chars = lower.chars();
    let base: i32 = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = if let Some(octave) = rest.strip_prefix('#') {
        (1, octave)
    } else if let Some(octave) = rest.strip_prefix('b').filter(|o| !o.is_empty()) {
        (-1, octave)
    } else {
        (0, rest)
    };
    let octave: i32 = octave.parse().ok()?;
    if !(-1..=9).contains(&octave) {
        return None;
    }
    let key = (octave + 1) * 12 + base + shift;
    u8::try_from(key).ok().filter(|k| *k <= 127)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    None,
    Control,
    Global,
    Master,
    Group,
    Region,
    /// `<curve>`, `<effect>`, `<midi>`, `<sample>` and anything unknown.
    Other,
}

impl Header {
    fn from_name(name: &str) -> Self {
        match name {
            "control" => Self::Control,
            "global" => Self::Global,
            "master" => Self::Master,
            "group" => Self::Group,
            "region" => Self::Region,
            _ => Self::Other,
        }
    }
}

/// The opcodes this player reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opcode {
    Sample,
    LoKey,
    HiKey,
    Key,
    LoVel,
    HiVel,
    LoRand,
    HiRand,
    SeqLength,
    SeqPosition,
    PitchKeycenter,
    PitchKeytrack,
    Transpose,
    Tune,
    Volume,
    Amplitude,
    Pan,
    AmpVeltrack,
    AmpegDelay,
    AmpegAttack,
    AmpegHold,
    AmpegDecay,
    AmpegSustain,
    AmpegRelease,
    Offset,
    End,
    LoopMode,
    LoopStart,
    LoopEnd,
    Trigger,
}

impl Opcode {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "sample" => Self::Sample,
            "lokey" => Self::LoKey,
            "hikey" => Self::HiKey,
            "key" => Self::Key,
            "lovel" => Self::LoVel,
            "hivel" => Self::HiVel,
            "lorand" => Self::LoRand,
            "hirand" => Self::HiRand,
            "seq_length" => Self::SeqLength,
            "seq_position" => Self::SeqPosition,
            "pitch_keycenter" => Self::PitchKeycenter,
            "pitch_keytrack" => Self::PitchKeytrack,
            "transpose" => Self::Transpose,
            "tune" => Self::Tune,
            "volume" => Self::Volume,
            "amplitude" => Self::Amplitude,
            "pan" => Self::Pan,
            "amp_veltrack" => Self::AmpVeltrack,
            "ampeg_delay" => Self::AmpegDelay,
            "ampeg_attack" => Self::AmpegAttack,
            "ampeg_hold" => Self::AmpegHold,
            "ampeg_decay" => Self::AmpegDecay,
            "ampeg_sustain" => Self::AmpegSustain,
            "ampeg_release" => Self::AmpegRelease,
            "offset" => Self::Offset,
            "end" => Self::End,
            "loop_mode" | "loopmode" => Self::LoopMode,
            "loop_start" | "loopstart" => Self::LoopStart,
            "loop_end" | "loopend" => Self::LoopEnd,
            "trigger" => Self::Trigger,
            _ => return None,
        })
    }

    /// Whether `value` can be applied; an invalid one is counted and left out.
    fn is_valid(self, value: &str) -> bool {
        let mut region = Region::default();
        self.apply(&mut region, value)
    }

    /// Sets the field of `region` this opcode stands for. False when the value is not valid.
    fn apply(self, region: &mut Region, value: &str) -> bool {
        let number = || value.parse::<f32>().ok().filter(|v| v.is_finite());
        let in_range = |low: f32, high: f32| number().map(|v| v.clamp(low, high));
        let frames = || value.parse::<u64>().ok();
        let count = || value.parse::<u32>().ok().filter(|v| *v >= 1);
        let velocity = || value.parse::<u8>().ok().filter(|v| *v <= 127);
        let env = &mut region.envelope;
        // Envelope times above a minute are clamped: they could only be mistakes.
        let seconds = || in_range(0.0, 60.0);
        let set = |field: &mut f32, value: Option<f32>| value.map(|v| *field = v).is_some();
        match self {
            Self::Sample => {
                region.sample = value.to_string();
                !value.is_empty()
            }
            Self::LoKey => parse_key(value).map(|k| region.lokey = k).is_some(),
            Self::HiKey => parse_key(value).map(|k| region.hikey = k).is_some(),
            Self::Key => parse_key(value)
                .map(|k| {
                    region.lokey = k;
                    region.hikey = k;
                    region.pitch_keycenter = k;
                })
                .is_some(),
            Self::LoVel => velocity().map(|v| region.lovel = v).is_some(),
            Self::HiVel => velocity().map(|v| region.hivel = v).is_some(),
            Self::LoRand => set(&mut region.lorand, in_range(0.0, 1.0)),
            Self::HiRand => set(&mut region.hirand, in_range(0.0, 1.0)),
            Self::SeqLength => count().map(|v| region.seq_length = v.min(100)).is_some(),
            Self::SeqPosition => count().map(|v| region.seq_position = v.min(100)).is_some(),
            Self::PitchKeycenter => parse_key(value)
                .map(|k| region.pitch_keycenter = k)
                .is_some(),
            Self::PitchKeytrack => set(&mut region.pitch_keytrack, in_range(-1200.0, 1200.0)),
            Self::Transpose => value
                .parse::<i32>()
                .ok()
                .filter(|v| (-127..=127).contains(v))
                .map(|v| region.transpose = v)
                .is_some(),
            Self::Tune => set(&mut region.tune, in_range(-9600.0, 9600.0)),
            Self::Volume => set(&mut region.volume, in_range(-144.0, 24.0)),
            Self::Amplitude => set(&mut region.amplitude, in_range(0.0, 100.0)),
            Self::Pan => set(&mut region.pan, in_range(-100.0, 100.0)),
            Self::AmpVeltrack => set(&mut region.amp_veltrack, in_range(-100.0, 100.0)),
            Self::AmpegDelay => set(&mut env.delay, seconds()),
            Self::AmpegAttack => set(&mut env.attack, seconds()),
            Self::AmpegHold => set(&mut env.hold, seconds()),
            Self::AmpegDecay => set(&mut env.decay, seconds()),
            Self::AmpegSustain => set(&mut env.sustain, in_range(0.0, 100.0)),
            Self::AmpegRelease => set(&mut env.release, seconds()),
            Self::Offset => frames().map(|v| region.offset = v).is_some(),
            Self::End => frames().map(|v| region.end = Some(v)).is_some(),
            Self::LoopStart => frames().map(|v| region.loop_start = Some(v)).is_some(),
            Self::LoopEnd => frames().map(|v| region.loop_end = Some(v)).is_some(),
            Self::LoopMode => {
                let mode = match value {
                    "no_loop" => LoopMode::NoLoop,
                    "one_shot" => LoopMode::OneShot,
                    "loop_continuous" => LoopMode::Continuous,
                    "loop_sustain" => LoopMode::Sustain,
                    _ => return false,
                };
                region.loop_mode = Some(mode);
                true
            }
            // Only notes that start a sound are played; a region triggered otherwise (on the
            // release, by the first or a legato note) is left out as a whole, see `finish_region`.
            Self::Trigger => {
                region.on_attack = match value {
                    "attack" => true,
                    "release" | "release_key" | "first" | "legato" => false,
                    _ => return false,
                };
                true
            }
        }
    }
}

/// Builds the region whose opcodes were read and adds it, unless it has no sample or is
/// triggered other than by a note-on.
fn finish_region(
    parsed: &mut Parsed,
    levels: [&Vec<(Opcode, String)>; 4],
    default_path: &str,
) -> Result<(), String> {
    let mut region = Region::default();
    let mut has_sample = false;
    for (opcode, value) in levels.into_iter().flatten() {
        // Every stored value was checked by `is_valid` when it was read.
        opcode.apply(&mut region, value);
        has_sample |= *opcode == Opcode::Sample;
    }
    if !has_sample {
        parsed.ignore("(region without sample)");
        return Ok(());
    }
    if !region.on_attack {
        parsed.ignore("(region not triggered by a note-on)");
        return Ok(());
    }
    if parsed.regions.len() >= MAX_REGIONS {
        return Err(format!("more than {MAX_REGIONS} regions"));
    }
    region.sample = format!("{default_path}{}", region.sample.replace('\\', "/"));
    parsed.regions.push(region);
    Ok(())
}

enum Token<'a> {
    Header(&'a str),
    Opcode(&'a str, &'a str),
    Junk,
}

/// Splits a line into headers and `name=value` opcodes. A value runs to the next opcode or
/// header, so a sample path may hold spaces.
fn tokens(line: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            match after.find('>') {
                Some(end) => {
                    out.push(Token::Header(after[..end].trim()));
                    rest = after[end + 1..].trim_start();
                }
                None => {
                    out.push(Token::Junk);
                    break;
                }
            }
            continue;
        }
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if name_len == 0 || !rest[name_len..].starts_with('=') {
            out.push(Token::Junk);
            // Skip to the next word.
            let skip = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest = rest[skip..].trim_start();
            continue;
        }
        let name = &rest[..name_len];
        let value_and_rest = &rest[name_len + 1..];
        let end = value_end(value_and_rest);
        out.push(Token::Opcode(name, value_and_rest[..end].trim()));
        rest = value_and_rest[end..].trim_start();
    }
    out
}

/// Where a value ends: at a `<`, or at a space followed by the next `name=`.
fn value_end(text: &str) -> usize {
    for (index, c) in text.char_indices() {
        if c == '<' {
            return index;
        }
        if c.is_whitespace() {
            let next = text[index..].trim_start();
            let name_len = next
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(next.len());
            if next.starts_with('<') || (name_len > 0 && next[name_len..].starts_with('=')) {
                return index;
            }
        }
    }
    text.len()
}

/// Replaces each `$NAME` defined, the longest name first when several match.
fn expand(line: &str, defines: &[(String, String)]) -> String {
    if defines.is_empty() || !line.contains('$') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let found = defines
            .iter()
            .filter(|(name, _)| tail.starts_with(name.as_str()))
            .max_by_key(|(name, _)| name.len());
        match found {
            Some((name, value)) => {
                out.push_str(value);
                rest = &tail[name.len()..];
            }
            None => {
                out.push('$');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes `//` line comments and `/* */` block comments, keeping the line breaks.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let line_comment = rest.find("//");
        let block_comment = rest.find("/*");
        match (line_comment, block_comment) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                rest = &rest[l..];
                rest = rest.find('\n').map_or("", |end| &rest[end..]);
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                let after = &rest[b + 2..];
                match after.find("*/") {
                    Some(end) => {
                        // Keep the line breaks so a block does not glue two lines together.
                        out.extend(after[..end].chars().filter(|c| *c == '\n'));
                        out.push(' ');
                        rest = &after[end + 2..];
                    }
                    None => rest = "",
                }
            }
            _ => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_and_numbers_read_as_midi_keys() {
        assert_eq!(parse_key("60"), Some(60));
        assert_eq!(parse_key("c4"), Some(60));
        assert_eq!(parse_key("C#4"), Some(61));
        assert_eq!(parse_key("db4"), Some(61));
        assert_eq!(parse_key("b3"), Some(59));
        assert_eq!(parse_key("c-1"), Some(0));
        assert_eq!(parse_key("g9"), Some(127));
        for bad in ["128", "-1", "h4", "c", "c10", "", "g#9"] {
            assert_eq!(parse_key(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_region_inherits_from_its_group_and_global_and_overrides_them() {
        let parsed = parse(
            "<global> volume=-6 ampeg_release=0.5\n\
             <group> lovel=1 hivel=64 volume=-3\n\
             <region> sample=soft.wav key=40\n\
             <region> sample=soft2.wav lokey=41 hikey=c3 pitch_keycenter=41 volume=0\n\
             <group> lovel=65\n\
             <region> sample=loud.wav key=40",
        )
        .unwrap();
        assert!(parsed.ignored.is_empty(), "{:?}", parsed.ignored);
        let [soft, soft2, loud] = parsed.regions.as_slice() else {
            panic!("{:?}", parsed.regions);
        };
        assert_eq!((soft.lokey, soft.hikey, soft.pitch_keycenter), (40, 40, 40));
        assert_eq!((soft.lovel, soft.hivel, soft.volume), (1, 64, -3.0));
        assert_eq!(soft.envelope.release, 0.5);
        assert_eq!((soft2.lokey, soft2.hikey, soft2.volume), (41, 48, 0.0));
        assert_eq!((loud.lovel, loud.hivel, loud.volume), (65, 127, -6.0));
    }

    #[test]
    fn a_sample_path_may_hold_spaces_and_backslashes() {
        let parsed = parse(
            "<control> default_path=Samples\\Nylon\\\n\
             <region> sample=Open E 1.wav lokey=40 hikey=44 // a comment\n\
             <region>sample=b.wav key=45",
        )
        .unwrap();
        assert_eq!(parsed.regions[0].sample, "Samples/Nylon/Open E 1.wav");
        assert_eq!(parsed.regions[0].hikey, 44);
        assert_eq!(parsed.regions[1].sample, "Samples/Nylon/b.wav");
    }

    #[test]
    fn unknown_opcodes_and_headers_are_counted_not_fatal() {
        let parsed = parse(
            "#include \"other.sfz\"\n\
             <curve> v000=0 v127=1\n\
             <group> sw_last=36 fil_type=lpf_2p\n\
             <region> sample=a.wav key=40 lokey=banana off_by=2\n\
             <region> sample=b.wav trigger=release\n\
             <region> key=41\n\
             /* a block\n comment */ <region> sample=c.wav",
        )
        .unwrap();
        // The release-triggered region and the one without a sample are left out.
        let samples: Vec<&str> = parsed.regions.iter().map(|r| r.sample.as_str()).collect();
        assert_eq!(samples, ["a.wav", "c.wav"]);
        for name in [
            "#include",
            "<curve>",
            "sw_last",
            "fil_type",
            "lokey=banana",
            "off_by",
            "(region not triggered by a note-on)",
            "(region without sample)",
        ] {
            assert_eq!(parsed.ignored.get(name), Some(&1), "{name}: {parsed:?}");
        }
    }

    #[test]
    fn defines_are_expanded() {
        let parsed =
            parse("#define $KEY 52\n#define $KEYS 99\n<region> sample=a.wav key=$KEY tune=$KEYS")
                .unwrap();
        assert_eq!(parsed.regions[0].lokey, 52);
        assert_eq!(parsed.regions[0].tune, 99.0);
    }

    #[test]
    fn round_robin_loops_and_envelope_are_read() {
        let parsed = parse(
            "<region> sample=a.wav seq_length=3 seq_position=2 loop_mode=loop_sustain \
             loopstart=10 loop_end=900 ampeg_attack=0.01 ampeg_sustain=40 offset=5 end=1000",
        )
        .unwrap();
        let region = &parsed.regions[0];
        assert_eq!((region.seq_length, region.seq_position), (3, 2));
        assert_eq!(region.loop_mode, Some(LoopMode::Sustain));
        assert_eq!((region.loop_start, region.loop_end), (Some(10), Some(900)));
        assert_eq!(region.envelope.attack, 0.01);
        assert_eq!(region.envelope.sustain, 40.0);
        assert_eq!((region.offset, region.end), (5, Some(1000)));
    }

    #[test]
    fn too_many_regions_is_an_error() {
        let text = "<region> sample=a.wav\n".repeat(MAX_REGIONS + 1);
        assert!(parse(&text).unwrap_err().contains("regions"));
    }

    #[test]
    fn a_define_cannot_blow_up_the_text() {
        let mut text =
            String::from("#define $A aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
        let line = format!("{}\n", "$A ".repeat(1000));
        for _ in 0..1000 {
            text.push_str(&line);
        }
        assert!(parse(&text).unwrap_err().contains("too large"));
    }

    #[test]
    fn garbage_is_counted_without_panicking() {
        for text in [
            "",
            "<",
            "<region",
            "=",
            "==== <> <region> sample=",
            "<region> sample=a.wav lokey=",
            "\u{0}\u{ffff} <région> ü=1",
            "/* never closed",
            "#define",
            "#define $",
        ] {
            let parsed = parse(text).unwrap();
            assert!(parsed.regions.len() <= 1, "{text}");
        }
    }
}
