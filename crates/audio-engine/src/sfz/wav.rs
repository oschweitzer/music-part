//! A small WAV reader: integer PCM (8, 16, 24, 32-bit) and float (32, 64-bit), mono or stereo,
//! with the first loop of a `smpl` chunk. Every size read from the file is checked against the
//! file length and a memory budget before anything is allocated.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

/// Highest sample rate accepted; above, the header is more likely wrong than the recording.
const MAX_SAMPLE_RATE: u32 = 384_000;
/// Chunks looked at before giving up on finding `fmt ` and `data`.
const MAX_CHUNKS: usize = 1024;
/// Bytes read from the file at a time while decoding.
const READ_BLOCK: usize = 64 * 1024;

/// A decoded sample: frames of `channels` interleaved values in -1 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub data: Vec<f32>,
    /// 1 or 2.
    pub channels: usize,
    pub sample_rate: u32,
    /// First and last frame of the loop the file declares, the last one included.
    pub loop_points: Option<(u64, u64)>,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels.max(1)
    }

    /// Bytes the decoded data takes in memory.
    pub fn bytes(&self) -> u64 {
        (self.data.len() * size_of::<f32>()) as u64
    }
}

/// Why a WAV file could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum WavError {
    Io(String),
    /// Not a RIFF WAVE file, or a broken one.
    Invalid(String),
    /// A WAV this reader does not decode (compressed, more than two channels...).
    Unsupported(String),
    /// Decoding would take more memory than the budget left.
    TooLarge {
        needed: u64,
        budget: u64,
    },
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot read: {e}"),
            Self::Invalid(e) => write!(f, "not a valid WAV file: {e}"),
            Self::Unsupported(e) => write!(f, "unsupported WAV: {e}"),
            Self::TooLarge { needed, budget } => write!(
                f,
                "needs {needed} bytes of memory, over the {budget} left for the pack"
            ),
        }
    }
}

impl std::error::Error for WavError {}

impl From<io::Error> for WavError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Decodes a WAV file. `budget` is the most bytes the decoded samples may take.
pub fn read<R: Read + Seek>(reader: &mut R, budget: u64) -> Result<Sample, WavError> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut riff = [0u8; 12];
    reader
        .read_exact(&mut riff)
        .map_err(|_| WavError::Invalid("too short".into()))?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(WavError::Invalid("no RIFF WAVE header".into()));
    }

    let mut format: Option<Format> = None;
    let mut data: Option<(u64, u64)> = None;
    let mut loop_points = None;
    let mut position = 12u64;
    for _ in 0..MAX_CHUNKS {
        if position + 8 > file_len {
            break;
        }
        reader.seek(SeekFrom::Start(position))?;
        let mut header = [0u8; 8];
        reader.read_exact(&mut header)?;
        let size = u64::from(u32_at(&header, 4));
        let body = position + 8;
        // Streaming writers leave the size of the last chunk at its maximum: trust the file.
        let available = size.min(file_len - body);
        match &header[0..4] {
            b"fmt " => format = Some(read_format(reader, available)?),
            b"data" => data = Some((body, available)),
            b"smpl" => loop_points = read_loop(reader, available)?,
            _ => {}
        }
        // Chunks are padded to an even size.
        position = body + size + (size & 1);
    }
    let format = format.ok_or_else(|| WavError::Invalid("no fmt chunk".into()))?;
    let (start, len) = data.ok_or_else(|| WavError::Invalid("no data chunk".into()))?;

    let frames = len / format.block_align as u64;
    let values = frames * format.channels as u64;
    let needed = values * size_of::<f32>() as u64;
    if needed > budget {
        return Err(WavError::TooLarge { needed, budget });
    }
    let values = usize::try_from(values).map_err(|_| WavError::TooLarge { needed, budget })?;
    let mut decoded = Vec::with_capacity(values);
    reader.seek(SeekFrom::Start(start))?;
    let bytes_per_value = format.block_align / format.channels;
    // Whole frames per read, so a value never straddles two reads.
    let mut block = vec![0u8; (READ_BLOCK / format.block_align).max(1) * format.block_align];
    while decoded.len() < values {
        let wanted = ((values - decoded.len()) * bytes_per_value).min(block.len());
        reader.read_exact(&mut block[..wanted])?;
        decoded.extend(
            block[..wanted]
                .chunks_exact(bytes_per_value)
                .map(|bytes| format.encoding.decode(bytes)),
        );
    }
    Ok(Sample {
        data: decoded,
        channels: format.channels,
        sample_rate: format.sample_rate,
        loop_points: loop_points.filter(|(first, last)| first < last && *last < frames),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Unsigned8,
    Signed16,
    Signed24,
    Signed32,
    Float32,
    Float64,
}

impl Encoding {
    /// Decodes one value of `bytes`, whose length is the size of this encoding.
    fn decode(self, bytes: &[u8]) -> f32 {
        let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
        match self {
            Self::Unsigned8 => (f32::from(byte(0)) - 128.0) / 128.0,
            Self::Signed16 => f32::from(i16::from_le_bytes([byte(0), byte(1)])) / 32_768.0,
            Self::Signed24 => {
                // Into the top three bytes of an i32, so the sign comes along.
                let value = i32::from_le_bytes([0, byte(0), byte(1), byte(2)]);
                value as f32 / 2_147_483_648.0
            }
            Self::Signed32 => {
                i32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]) as f32 / 2_147_483_648.0
            }
            Self::Float32 => sanitize(f32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)])),
            Self::Float64 => sanitize(f64::from_le_bytes([
                byte(0),
                byte(1),
                byte(2),
                byte(3),
                byte(4),
                byte(5),
                byte(6),
                byte(7),
            ]) as f32),
        }
    }
}

/// A float sample can hold anything: keep it a number in the usual range.
fn sanitize(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

struct Format {
    encoding: Encoding,
    channels: usize,
    sample_rate: u32,
    /// Bytes per frame.
    block_align: usize,
}

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

fn read_format<R: Read>(reader: &mut R, size: u64) -> Result<Format, WavError> {
    if size < 16 {
        return Err(WavError::Invalid("fmt chunk too short".into()));
    }
    let mut bytes = [0u8; 40];
    let len = size.min(40) as usize;
    reader.read_exact(&mut bytes[..len])?;
    let mut tag = u16_at(&bytes, 0);
    let channels = usize::from(u16_at(&bytes, 2));
    let sample_rate = u32_at(&bytes, 4);
    let block_align = usize::from(u16_at(&bytes, 12));
    let bits = u16_at(&bytes, 14);
    if tag == FORMAT_EXTENSIBLE {
        if len < 40 {
            return Err(WavError::Invalid("extensible fmt chunk too short".into()));
        }
        // The first two bytes of the sub-format GUID are the format tag.
        tag = u16_at(&bytes, 24);
    }
    let encoding = match (tag, bits) {
        (FORMAT_PCM, 8) => Encoding::Unsigned8,
        (FORMAT_PCM, 16) => Encoding::Signed16,
        (FORMAT_PCM, 24) => Encoding::Signed24,
        (FORMAT_PCM, 32) => Encoding::Signed32,
        (FORMAT_FLOAT, 32) => Encoding::Float32,
        (FORMAT_FLOAT, 64) => Encoding::Float64,
        _ => {
            return Err(WavError::Unsupported(format!(
                "format {tag} with {bits} bits"
            )))
        }
    };
    if !(1..=2).contains(&channels) {
        return Err(WavError::Unsupported(format!("{channels} channels")));
    }
    if !(1..=MAX_SAMPLE_RATE).contains(&sample_rate) {
        return Err(WavError::Invalid(format!("sample rate {sample_rate}")));
    }
    if block_align != channels * usize::from(bits / 8) {
        return Err(WavError::Invalid(format!(
            "{block_align} bytes per frame for {channels} channels of {bits} bits"
        )));
    }
    Ok(Format {
        encoding,
        channels,
        sample_rate,
        block_align,
    })
}

/// The first loop of a `smpl` chunk, if any.
fn read_loop<R: Read>(reader: &mut R, size: u64) -> Result<Option<(u64, u64)>, WavError> {
    // 36 bytes of header, then 24 bytes per loop: cue id, type, start, end, fraction, count.
    if size < 36 + 24 {
        return Ok(None);
    }
    let mut bytes = [0u8; 60];
    reader.read_exact(&mut bytes)?;
    if u32_at(&bytes, 28) == 0 {
        return Ok(None);
    }
    let first = u64::from(u32_at(&bytes, 36 + 8));
    let last = u64::from(u32_at(&bytes, 36 + 12));
    Ok(Some((first, last)))
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Writes a 16-bit WAV file, for tests that need sample files: there are no binary fixtures in
/// the repository.
#[cfg(test)]
pub(crate) fn encode_16bit(
    channels: u16,
    sample_rate: u32,
    values: &[f32],
    loop_points: Option<(u32, u32)>,
) -> Vec<u8> {
    let data_len = (values.len() * 2) as u32;
    let smpl_len: u32 = if loop_points.is_some() { 60 } else { 0 };
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    let riff_len = 4 + 8 + 16 + 8 + data_len + if smpl_len > 0 { 8 + smpl_len } else { 0 };
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&FORMAT_PCM.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    if let Some((first, last)) = loop_points {
        out.extend_from_slice(b"smpl");
        out.extend_from_slice(&smpl_len.to_le_bytes());
        let mut chunk = [0u8; 60];
        chunk[28..32].copy_from_slice(&1u32.to_le_bytes());
        chunk[44..48].copy_from_slice(&first.to_le_bytes());
        chunk[48..52].copy_from_slice(&last.to_le_bytes());
        out.extend_from_slice(&chunk);
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for value in values {
        let value = (value.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const BUDGET: u64 = 1 << 20;

    #[test]
    fn a_16_bit_file_decodes_to_its_values_and_loop() {
        let values = [0.0, 0.5, -0.5, 1.0, -1.0, 0.25];
        let bytes = encode_16bit(2, 22_050, &values, Some((0, 2)));
        let sample = read(&mut Cursor::new(bytes), BUDGET).unwrap();
        assert_eq!((sample.channels, sample.sample_rate), (2, 22_050));
        assert_eq!(sample.frames(), 3);
        assert_eq!(sample.loop_points, Some((0, 2)));
        for (got, want) in sample.data.iter().zip(values) {
            assert!((got - want).abs() < 1e-3, "{got} {want}");
        }
    }

    #[test]
    fn a_24_bit_and_a_float_file_decode() {
        let mut header = encode_16bit(1, 44_100, &[], None);
        // Turn it into 24-bit mono with one value of -0.5.
        header[34] = 24;
        header[32] = 3;
        header.truncate(header.len() - 4);
        header.extend_from_slice(&3u32.to_le_bytes());
        header.extend_from_slice(&[0x00, 0x00, 0xC0]);
        let sample = read(&mut Cursor::new(header), BUDGET).unwrap();
        assert_eq!(sample.data, vec![-0.5]);

        let mut float = encode_16bit(1, 44_100, &[], None);
        float[20] = 3;
        float[34] = 32;
        float[32] = 4;
        float.truncate(float.len() - 4);
        float.extend_from_slice(&8u32.to_le_bytes());
        float.extend_from_slice(&0.25f32.to_le_bytes());
        float.extend_from_slice(&f32::NAN.to_le_bytes());
        let sample = read(&mut Cursor::new(float), BUDGET).unwrap();
        assert_eq!(sample.data, vec![0.25, 0.0]);
    }

    #[test]
    fn a_file_over_the_budget_is_refused_before_decoding() {
        let bytes = encode_16bit(1, 44_100, &[0.1; 1000], None);
        let error = read(&mut Cursor::new(bytes), 3999).unwrap_err();
        assert_eq!(
            error,
            WavError::TooLarge {
                needed: 4000,
                budget: 3999
            }
        );
    }

    #[test]
    fn a_data_size_larger_than_the_file_is_cut_to_the_file() {
        let mut bytes = encode_16bit(1, 44_100, &[0.5; 10], None);
        let at = bytes.len() - 20 - 4;
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let sample = read(&mut Cursor::new(bytes), BUDGET).unwrap();
        assert_eq!(sample.frames(), 10);
    }

    #[test]
    fn broken_or_unsupported_files_are_errors() {
        let good = encode_16bit(1, 44_100, &[0.5; 10], None);
        let mut cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"RIFF".to_vec(),
            b"RIFF\0\0\0\0WAVX".to_vec(),
            good[..30].to_vec(),
            good[..44 - 8].to_vec(),
        ];
        let mut three_channels = good.clone();
        three_channels[22] = 3;
        cases.push(three_channels);
        let mut compressed = good.clone();
        compressed[20] = 2;
        cases.push(compressed);
        let mut zero_rate = good.clone();
        zero_rate[24..28].copy_from_slice(&0u32.to_le_bytes());
        cases.push(zero_rate);
        let mut bad_align = good.clone();
        bad_align[32] = 7;
        cases.push(bad_align);
        for (index, case) in cases.into_iter().enumerate() {
            assert!(
                read(&mut Cursor::new(case), BUDGET).is_err(),
                "case {index}"
            );
        }
    }

    #[test]
    fn random_bytes_never_panic() {
        let good = encode_16bit(2, 44_100, &[0.5; 64], Some((1, 30)));
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..2000 {
            let mut bytes = good.clone();
            for _ in 0..(next() % 8 + 1) {
                let at = next() as usize % bytes.len();
                bytes[at] = next() as u8;
            }
            bytes.truncate(next() as usize % (bytes.len() + 1));
            let _ = read(&mut Cursor::new(bytes), BUDGET);
        }
    }
}
