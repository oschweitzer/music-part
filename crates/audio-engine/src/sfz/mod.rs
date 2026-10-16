//! SFZ instruments: a text file of regions and the WAV samples it names, played by a pool of
//! voices mixed with the synthesizer. A subset of the format, chosen for multi-sampled string
//! instruments; what it reads and what it ignores is in `docs/sfz.md`.

pub mod parse;
pub mod voice;
pub mod wav;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::RangeInclusive;
use std::path::{Component, Path, PathBuf};

use score_model::Sound;

use parse::{LoopMode, Region};
pub use voice::{NoteOn, Voices, MAX_VOICES, SFZ_GAIN};

/// Most bytes of an `.sfz` file read.
pub const MAX_SFZ_BYTES: u64 = 1024 * 1024;
/// Most distinct sample files an `.sfz` file may name.
pub const MAX_SAMPLE_FILES: usize = 2048;
/// Most memory the decoded samples of one pack may take, all its `.sfz` files together.
pub const MAX_PACK_SAMPLE_BYTES: u64 = 256 * 1024 * 1024;

/// An SFZ file loaded with its samples, ready to play.
#[derive(Debug, Clone)]
pub struct SfzInstrument {
    regions: Vec<PlayRegion>,
    samples: Vec<wav::Sample>,
    /// Round-robin counter of each region, allocated here so that playing a note does not.
    seq_counters: Vec<u32>,
}

impl SfzInstrument {
    /// Whether region `index` plays a note, moving its round-robin counter on when its key,
    /// velocity and random range match.
    fn answers(&mut self, index: usize, key: u8, velocity: u8, random: f32) -> bool {
        let Some(region) = self.regions.get(index) else {
            return false;
        };
        let spec = &region.spec;
        let matches = (spec.lokey..=spec.hikey).contains(&key)
            && (spec.lovel..=spec.hivel).contains(&velocity)
            && spec.lorand <= random
            && (random < spec.hirand || spec.hirand >= 1.0);
        if !matches {
            return false;
        }
        let Some(counter) = self.seq_counters.get_mut(index) else {
            return false;
        };
        let turn = *counter % spec.seq_length.max(1);
        *counter = counter.wrapping_add(1);
        turn + 1 == spec.seq_position
    }

    /// Bytes its decoded samples take.
    pub fn sample_bytes(&self) -> u64 {
        self.samples.iter().map(wav::Sample::bytes).sum()
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Whether a region plays `key` at some velocity, whatever its round-robin turn.
    pub fn covers(&self, key: u8) -> bool {
        self.regions
            .iter()
            .any(|region| (region.spec.lokey..=region.spec.hikey).contains(&key))
    }
}

/// A region ready to play: its opcodes, its sample, and its loop worked out against the sample.
#[derive(Debug, Clone)]
struct PlayRegion {
    spec: Region,
    /// Index in `SfzInstrument::samples`.
    sample: usize,
    channels: usize,
    first_frame: u64,
    last_frame: u64,
    loop_mode: LoopMode,
    loop_first: u64,
    loop_last: u64,
}

impl PlayRegion {
    /// The loop, first frame and the frame after the last.
    fn loop_range(&self) -> Option<(f64, f64)> {
        (self.loop_first < self.loop_last)
            .then_some((self.loop_first as f64, (self.loop_last + 1) as f64))
    }
}

/// An SFZ instrument given to a sound, in the `Player`.
#[derive(Debug, Clone)]
pub struct SfzSound {
    pub sound: Sound,
    /// Keys the pack declares; a note outside is played by the General MIDI program.
    pub keys: RangeInclusive<u8>,
    pub instrument: SfzInstrument,
}

/// What loading an `.sfz` file left out, for the console.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SfzReport {
    pub regions: usize,
    pub samples: usize,
    pub sample_bytes: u64,
    /// Opcodes, headers and directives not supported, with how many times each was met.
    pub ignored: BTreeMap<String, u32>,
    /// Regions left out, by reason (the sample is missing, broken, too large...).
    pub dropped: BTreeMap<String, u32>,
}

impl SfzReport {
    pub fn is_complete(&self) -> bool {
        self.ignored.is_empty() && self.dropped.is_empty()
    }
}

impl fmt::Display for SfzReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} regions, {} samples, {:.1} MB",
            self.regions,
            self.samples,
            self.sample_bytes as f64 / 1_000_000.0
        )?;
        let list = |map: &BTreeMap<String, u32>| {
            map.iter()
                .map(|(name, count)| format!("{count} × {name}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !self.ignored.is_empty() {
            write!(f, "; ignored: {}", list(&self.ignored))?;
        }
        if !self.dropped.is_empty() {
            write!(f, "; regions left out: {}", list(&self.dropped))?;
        }
        Ok(())
    }
}

/// Why an `.sfz` file could not be used at all.
#[derive(Debug, Clone, PartialEq)]
pub enum SfzError {
    Io(String),
    /// A limit of `docs/sfz.md` was passed.
    TooLarge(String),
    /// A path leads out of the pack folder.
    OutsidePack(String),
    /// Not one region could be played.
    NoRegion(SfzReport),
}

impl fmt::Display for SfzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read: {e}"),
            Self::TooLarge(e) => write!(f, "too large: {e}"),
            Self::OutsidePack(path) => write!(f, "{path:?} is outside the pack folder"),
            Self::NoRegion(report) => write!(f, "no region can be played ({report})"),
        }
    }
}

impl std::error::Error for SfzError {}

/// Loads the `.sfz` file at `path` and decodes its samples, which must all be inside `pack_dir`.
/// `budget` is the memory the samples may still take, lowered by what they take. A region whose
/// sample cannot be used is left out and counted in the report; a path out of the pack refuses
/// the whole file.
pub fn load(
    path: &Path,
    pack_dir: &Path,
    budget: &mut u64,
) -> Result<(SfzInstrument, SfzReport), SfzError> {
    let io = |e: std::io::Error| SfzError::Io(e.to_string());
    let pack_dir = pack_dir.canonicalize().map_err(io)?;
    let path = path.canonicalize().map_err(io)?;
    if !path.starts_with(&pack_dir) {
        return Err(SfzError::OutsidePack(path.display().to_string()));
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .map_err(io)?
        .take(MAX_SFZ_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > MAX_SFZ_BYTES {
        return Err(SfzError::TooLarge(format!(
            "the file is over {MAX_SFZ_BYTES} bytes"
        )));
    }
    // Some SFZ files are Latin-1: keep what is readable rather than refuse them.
    let text = String::from_utf8_lossy(&bytes);
    let parsed = parse::parse(&text).map_err(SfzError::TooLarge)?;
    let sfz_dir = path.parent().unwrap_or(&pack_dir).to_path_buf();

    // Check every path before reading any sample, so a pack that reaches out reads nothing.
    let mut resolved = Vec::with_capacity(parsed.regions.len());
    for region in &parsed.regions {
        resolved.push(resolve(&pack_dir, &sfz_dir, &region.sample)?);
    }

    let mut report = SfzReport {
        ignored: parsed.ignored,
        ..SfzReport::default()
    };
    let mut samples: Vec<wav::Sample> = Vec::new();
    // Each file is read once, however many regions use it.
    let mut by_path: BTreeMap<PathBuf, Result<usize, String>> = BTreeMap::new();
    let mut regions = Vec::new();
    for (spec, target) in parsed.regions.into_iter().zip(resolved) {
        let sample = match target {
            Target::Generated => Err("generated sample (*sine...)".to_string()),
            Target::File(file) => match by_path.get(&file) {
                Some(known) => known.clone(),
                None => {
                    let read = if by_path.len() >= MAX_SAMPLE_FILES {
                        Err(format!("more than {MAX_SAMPLE_FILES} sample files"))
                    } else {
                        read_sample(&file, budget).map(|sample| {
                            samples.push(sample);
                            samples.len() - 1
                        })
                    };
                    by_path.insert(file, read.clone());
                    read
                }
            },
        };
        let built = sample.and_then(|index| play_region(spec, index, &samples[index]));
        match built {
            Ok(region) => regions.push(region),
            Err(reason) => *report.dropped.entry(reason).or_insert(0) += 1,
        }
    }
    report.regions = regions.len();
    report.samples = samples.len();
    report.sample_bytes = samples.iter().map(wav::Sample::bytes).sum();
    if regions.is_empty() {
        return Err(SfzError::NoRegion(report));
    }
    let seq_counters = vec![0; regions.len()];
    Ok((
        SfzInstrument {
            regions,
            samples,
            seq_counters,
        },
        report,
    ))
}

/// Where a region's sample comes from.
enum Target {
    File(PathBuf),
    /// `*sine`, `*silence` and the like: not supported.
    Generated,
}

/// The file a `sample` path (with `default_path`, `\` read as `/`) names, relative to the `.sfz`
/// file. Refuses absolute paths, drive letters, `..`, and a file that a link puts outside the
/// pack.
fn resolve(pack_dir: &Path, sfz_dir: &Path, sample: &str) -> Result<Target, SfzError> {
    if sample.starts_with('*') {
        return Ok(Target::Generated);
    }
    let outside = || SfzError::OutsidePack(sample.to_string());
    let relative = sample.replace('\\', "/");
    if relative.starts_with('/') || relative.contains(':') {
        return Err(outside());
    }
    let relative = Path::new(&relative);
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(outside());
    }
    let joined = sfz_dir.join(relative);
    match joined.canonicalize() {
        Ok(real) if real.starts_with(pack_dir) => Ok(Target::File(real)),
        Ok(_) => Err(outside()),
        // Missing: reported when it is read, as for any unreadable sample.
        Err(_) => Ok(Target::File(joined)),
    }
}

fn read_sample(path: &Path, budget: &mut u64) -> Result<wav::Sample, String> {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let file = File::open(path).map_err(|_| format!("missing sample {name}"))?;
    let sample = wav::read(&mut BufReader::new(file), *budget).map_err(|e| match e {
        wav::WavError::TooLarge { .. } => {
            format!("sample {name} over the pack memory limit")
        }
        other => format!("sample {name}: {other}"),
    })?;
    *budget = budget.saturating_sub(sample.bytes());
    Ok(sample)
}

fn play_region(spec: Region, sample: usize, data: &wav::Sample) -> Result<PlayRegion, String> {
    let frames = data.frames() as u64;
    if frames == 0 {
        return Err("empty sample".into());
    }
    let last_frame = spec.end.unwrap_or(u64::MAX).min(frames - 1);
    if spec.offset > last_frame {
        return Err("offset past the end of the sample".into());
    }
    let mode = spec.loop_mode.unwrap_or(if data.loop_points.is_some() {
        LoopMode::Continuous
    } else {
        LoopMode::NoLoop
    });
    let (file_first, file_last) = data.loop_points.unwrap_or((0, last_frame));
    let loop_first = spec.loop_start.unwrap_or(file_first).min(last_frame);
    let loop_last = spec.loop_end.unwrap_or(file_last).min(last_frame);
    let loops = matches!(mode, LoopMode::Continuous | LoopMode::Sustain);
    Ok(PlayRegion {
        // A loop with nothing in it plays once.
        loop_mode: if loops && loop_first >= loop_last {
            LoopMode::NoLoop
        } else {
            mode
        },
        first_frame: spec.offset,
        last_frame,
        loop_first,
        loop_last,
        channels: data.channels,
        sample,
        spec,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sequencer::{Timbre, BEND_CENTER};
    use std::fs;

    /// A fresh empty folder under `target/`, so tests stay inside the repository.
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-sfz")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A mono 44.1 kHz sine of `seconds` at `hz` and amplitude `level`.
    pub(crate) fn sine_wav(hz: f32, level: f32, seconds: f32) -> Vec<u8> {
        let frames = (44_100.0 * seconds) as usize;
        let values: Vec<f32> = (0..frames)
            .map(|i| level * (std::f32::consts::TAU * hz * i as f32 / 44_100.0).sin())
            .collect();
        wav::encode_16bit(1, 44_100, &values, None)
    }

    /// A pack folder with an `.sfz` file and WAV files: `(name, bytes)`.
    pub(crate) fn write_sfz(dir: &Path, sfz: &str, wavs: &[(&str, Vec<u8>)]) -> PathBuf {
        for (name, bytes) in wavs {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let path = dir.join("test.sfz");
        fs::write(&path, sfz).unwrap();
        path
    }

    fn load_in(dir: &Path, path: &Path) -> Result<(SfzInstrument, SfzReport), SfzError> {
        let mut budget = MAX_PACK_SAMPLE_BYTES;
        load(path, dir, &mut budget)
    }

    /// Which sample file each region of a note plays, by index in the file's region list.
    fn regions_played(instrument: &mut SfzInstrument, key: u8, velocity: u8) -> Vec<usize> {
        (0..instrument.regions.len())
            .filter(|&i| instrument.answers(i, key, velocity, 0.5))
            .collect()
    }

    #[test]
    fn regions_are_chosen_by_key_and_velocity() {
        let dir = temp_dir("select");
        let path = write_sfz(
            &dir,
            "<group> lokey=40 hikey=50 pitch_keycenter=45\n\
             <region> sample=soft.wav hivel=63\n\
             <region> sample=loud.wav lovel=64\n\
             <region> sample=high.wav lokey=51 hikey=60",
            &[
                ("soft.wav", sine_wav(110.0, 0.5, 0.1)),
                ("loud.wav", sine_wav(110.0, 0.5, 0.1)),
                ("high.wav", sine_wav(220.0, 0.5, 0.1)),
            ],
        );
        let (mut instrument, report) = load_in(&dir, &path).unwrap();
        assert!(report.is_complete(), "{report}");
        assert_eq!((report.regions, report.samples), (3, 3));

        assert_eq!(regions_played(&mut instrument, 45, 30), [0]);
        assert_eq!(regions_played(&mut instrument, 45, 100), [1]);
        assert_eq!(regions_played(&mut instrument, 55, 100), [2]);
        assert!(regions_played(&mut instrument, 39, 100).is_empty());
        assert!(regions_played(&mut instrument, 61, 100).is_empty());
    }

    #[test]
    fn round_robin_takes_each_region_in_turn() {
        let dir = temp_dir("round-robin");
        let path = write_sfz(
            &dir,
            "<group> key=40 seq_length=3\n\
             <region> sample=a.wav seq_position=1\n\
             <region> sample=b.wav seq_position=2\n\
             <region> sample=c.wav seq_position=3",
            &[
                ("a.wav", sine_wav(100.0, 0.5, 0.05)),
                ("b.wav", sine_wav(100.0, 0.5, 0.05)),
                ("c.wav", sine_wav(100.0, 0.5, 0.05)),
            ],
        );
        let (mut instrument, _) = load_in(&dir, &path).unwrap();
        let turns: Vec<Vec<usize>> = (0..4)
            .map(|_| regions_played(&mut instrument, 40, 100))
            .collect();
        assert_eq!(turns, [vec![0], vec![1], vec![2], vec![0]]);
    }

    #[test]
    fn a_missing_or_broken_sample_drops_its_region_only() {
        let dir = temp_dir("broken-sample");
        let path = write_sfz(
            &dir,
            "<region> sample=good.wav key=40\n\
             <region> sample=missing.wav key=41\n\
             <region> sample=broken.wav key=42\n\
             <region> sample=*sine key=43",
            &[
                ("good.wav", sine_wav(100.0, 0.5, 0.05)),
                ("broken.wav", b"RIFF\0\0\0\0WAVEjunk".to_vec()),
            ],
        );
        let (instrument, report) = load_in(&dir, &path).unwrap();
        assert_eq!(instrument.region_count(), 1);
        assert_eq!(report.dropped.len(), 3, "{report}");
        assert!(report.dropped.contains_key("missing sample missing.wav"));
        assert!(report.to_string().contains("broken.wav"), "{report}");
    }

    #[test]
    fn an_sfz_without_a_playable_region_is_refused() {
        let dir = temp_dir("no-region");
        let path = write_sfz(&dir, "<region> sample=missing.wav", &[]);
        assert!(matches!(
            load_in(&dir, &path),
            Err(SfzError::NoRegion(report)) if report.dropped.len() == 1
        ));
    }

    #[test]
    fn a_sample_path_leaving_the_pack_is_refused() {
        let root = temp_dir("escape");
        let dir = root.join("pack");
        fs::create_dir_all(&dir).unwrap();
        fs::write(root.join("secret.wav"), sine_wav(100.0, 0.5, 0.05)).unwrap();
        fs::write(dir.join("ok.wav"), sine_wav(100.0, 0.5, 0.05)).unwrap();
        let escapes = [
            "<region> sample=../secret.wav",
            "<region> sample=..\\secret.wav",
            "<region> sample=sub/../../secret.wav",
            "<control> default_path=../ <region> sample=secret.wav",
            "<region> sample=/etc/passwd",
            "<region> sample=C:\\Windows\\win.ini",
            "<region> sample=\\\\server\\share\\a.wav",
        ];
        for sfz in escapes {
            let path = write_sfz(&dir, &format!("<region> sample=ok.wav\n{sfz}"), &[]);
            assert!(
                matches!(load_in(&dir, &path), Err(SfzError::OutsidePack(_))),
                "{sfz} was accepted"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_pack_is_refused() {
        let root = temp_dir("link");
        let dir = root.join("pack");
        fs::create_dir_all(&dir).unwrap();
        fs::write(root.join("secret.wav"), sine_wav(100.0, 0.5, 0.05)).unwrap();
        std::os::unix::fs::symlink(root.join("secret.wav"), dir.join("link.wav")).unwrap();
        let path = write_sfz(&dir, "<region> sample=link.wav", &[]);
        assert!(matches!(
            load_in(&dir, &path),
            Err(SfzError::OutsidePack(_))
        ));
    }

    #[test]
    fn samples_over_the_pack_budget_are_left_out() {
        let dir = temp_dir("budget");
        let path = write_sfz(
            &dir,
            "<region> sample=a.wav key=40\n<region> sample=b.wav key=41",
            &[
                ("a.wav", sine_wav(100.0, 0.5, 0.1)),
                ("b.wav", sine_wav(100.0, 0.5, 0.1)),
            ],
        );
        // Room for one of the two samples (4410 frames of 4 bytes each).
        let mut budget = 4410 * 4 + 100;
        let (instrument, report) = load(&path, &dir, &mut budget).unwrap();
        assert_eq!(instrument.region_count(), 1);
        assert_eq!(budget, 100);
        assert_eq!(
            report
                .dropped
                .get("sample b.wav over the pack memory limit"),
            Some(&1)
        );
    }

    #[test]
    fn a_sample_used_by_several_regions_is_read_once() {
        let dir = temp_dir("shared");
        let path = write_sfz(
            &dir,
            "<region> sample=a.wav key=40\n<region> sample=./a.wav key=41 transpose=1",
            &[("a.wav", sine_wav(100.0, 0.5, 0.1))],
        );
        let (instrument, report) = load_in(&dir, &path).unwrap();
        assert_eq!((report.regions, report.samples), (2, 1));
        assert_eq!(instrument.sample_bytes(), 4410 * 4);
    }

    fn sound_of(instrument: SfzInstrument) -> Vec<SfzSound> {
        vec![SfzSound {
            sound: Sound::GUITAR_NYLON,
            keys: 0..=127,
            instrument,
        }]
    }

    fn note(key: u8, timbre: Timbre) -> NoteOn {
        NoteOn {
            track: 0,
            key,
            velocity: 127,
            timbre,
            sample_rate: 44_100,
        }
    }

    fn peak(voices: &mut Voices, sounds: &[SfzSound], frames: usize) -> f32 {
        (0..frames).fold(0.0f32, |m, _| {
            let (l, r) = voices.next_frame(sounds);
            m.max(l.abs()).max(r.abs())
        })
    }

    #[test]
    fn a_looped_sample_sustains_and_a_released_note_fades_out() {
        let dir = temp_dir("loop");
        let path = write_sfz(
            &dir,
            "<region> sample=a.wav key=60 loop_mode=loop_continuous loop_start=0 \
             loop_end=99 ampeg_release=0.05",
            // 100 frames: one period of 441 Hz, looped for as long as the note is held.
            &[("a.wav", sine_wav(441.0, 1.0, 100.0 / 44_100.0))],
        );
        let (instrument, _) = load_in(&dir, &path).unwrap();
        let mut sounds = sound_of(instrument);
        let mut voices = Voices::new();
        assert!(voices.note_on(&mut sounds, 0, note(60, Timbre::Open)));
        // Two seconds after the start the loop still sounds, at the full SFZ gain.
        peak(&mut voices, &sounds, 88_200);
        let held = peak(&mut voices, &sounds, 441);
        assert!((held - SFZ_GAIN).abs() < 0.02, "{held}");

        voices.note_off(0, 60);
        peak(&mut voices, &sounds, 44_100 / 10);
        assert_eq!(voices.active(), 0, "the release is over after 50 ms");
    }

    #[test]
    fn a_palm_muted_note_is_duller_and_stops_sooner() {
        let dir = temp_dir("muted");
        let path = write_sfz(
            &dir,
            "<region> sample=a.wav key=60 ampeg_release=1",
            &[("a.wav", sine_wav(8_000.0, 1.0, 1.0))],
        );
        let (instrument, _) = load_in(&dir, &path).unwrap();
        let mut sounds = sound_of(instrument);
        let level = |sounds: &mut Vec<SfzSound>, timbre| {
            let mut voices = Voices::new();
            voices.note_on(sounds, 0, note(60, timbre));
            let held = peak(&mut voices, sounds, 4410);
            voices.note_off(0, 60);
            let after = {
                peak(&mut voices, sounds, 4410);
                peak(&mut voices, sounds, 441)
            };
            (held, after)
        };
        let (open, open_after) = level(&mut sounds, Timbre::Open);
        let (muted, muted_after) = level(&mut sounds, Timbre::Muted);
        assert!(
            muted < open * 0.3,
            "8 kHz through the mute: {muted} vs {open}"
        );
        assert!(
            muted_after < open_after * 0.01,
            "{muted_after} vs {open_after}"
        );
    }

    #[test]
    fn the_pitch_follows_the_key_and_the_wheel() {
        let dir = temp_dir("pitch");
        // A ramp from 0 to 1 over 1000 frames: the voice's position is its value.
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        fs::write(
            dir.join("ramp.wav"),
            wav::encode_16bit(1, 44_100, &ramp, None),
        )
        .unwrap();
        let path = write_sfz(
            &dir,
            "<region> sample=ramp.wav pitch_keycenter=60 amp_veltrack=0 pan=-100",
            &[],
        );
        let (instrument, _) = load_in(&dir, &path).unwrap();
        let mut sounds = sound_of(instrument);
        let mut after_100_frames = |key: u8, bend: u16| {
            let mut voices = Voices::new();
            voices.set_bend(0, bend);
            voices.note_on(&mut sounds, 0, note(key, Timbre::Open));
            for _ in 0..100 {
                voices.next_frame(&sounds);
            }
            // Left only, at the gain of a hard left pan.
            voices.next_frame(&sounds).0 / (SFZ_GAIN * std::f32::consts::SQRT_2)
        };
        let close = |a: f32, b: f32| (a - b).abs() < 0.003;
        assert!(close(after_100_frames(60, BEND_CENTER), 0.1));
        assert!(close(after_100_frames(72, BEND_CENTER), 0.2));
        // A wheel at the top is an octave up (12 semitones of range).
        assert!(close(after_100_frames(60, 16_383), 0.2));
    }

    #[test]
    fn the_pool_takes_over_the_oldest_voice_when_full() {
        let dir = temp_dir("pool");
        let path = write_sfz(
            &dir,
            "<region> sample=a.wav loop_mode=loop_continuous",
            &[("a.wav", sine_wav(100.0, 0.5, 0.1))],
        );
        let (instrument, _) = load_in(&dir, &path).unwrap();
        let mut sounds = sound_of(instrument);
        let mut voices = Voices::new();
        for key in 0..(MAX_VOICES as u8 + 10) {
            assert!(voices.note_on(&mut sounds, 0, note(key, Timbre::Open)));
        }
        assert_eq!(voices.active(), MAX_VOICES);
        // The first ten keys were taken over: their note-off finds nothing.
        voices.note_off(0, 0);
        assert_eq!(voices.active(), MAX_VOICES);
        voices.note_off(0, MAX_VOICES as u8 + 9);
        peak(&mut voices, &sounds, 4410);
        assert_eq!(voices.active(), MAX_VOICES - 1);
    }
}
