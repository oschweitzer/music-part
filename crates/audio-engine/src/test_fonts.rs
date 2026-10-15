//! Tiny SoundFonts built in memory, so tests can tell sample banks apart by the name of their
//! preset without committing binary files or fetching the real packs.

/// A SoundFont 2 with one preset (bank 0, program 0) named `preset`: a looped sine wave over the
/// whole keyboard.
pub(crate) fn tiny_soundfont(preset: &str) -> Vec<u8> {
    const LENGTH: u32 = 256;
    // A sample is followed by at least 46 zero samples (SoundFont 2.04, 6.1).
    let mut smpl = Vec::new();
    for i in 0..LENGTH + 46 {
        let value = if i < LENGTH {
            ((f64::from(i) * std::f64::consts::TAU / 32.0).sin() * 16_000.0) as i16
        } else {
            0
        };
        smpl.extend_from_slice(&value.to_le_bytes());
    }

    let info = list(
        b"INFO",
        &[
            chunk(b"ifil", &[2, 0, 1, 0]),
            chunk(b"isng", b"EMU8000\0"),
            chunk(b"INAM", b"Test\0\0"),
        ],
    );
    let sdta = list(b"sdta", &[chunk(b"smpl", &smpl)]);

    let phdr = [preset_header(preset, 0), preset_header("EOP", 1)].concat();
    let bag = |generator: u16| [generator.to_le_bytes(), 0u16.to_le_bytes()].concat();
    let generator = |oper: u16, amount: u16| [oper.to_le_bytes(), amount.to_le_bytes()].concat();
    // Generator 41 picks the instrument, 54 the loop mode (1: loop), 53 the sample.
    let pgen = [generator(41, 0), generator(0, 0)].concat();
    let igen = [generator(54, 1), generator(53, 0), generator(0, 0)].concat();
    let inst = [
        [name(preset).as_slice(), &0u16.to_le_bytes()].concat(),
        [name("EOI").as_slice(), &1u16.to_le_bytes()].concat(),
    ]
    .concat();
    let shdr = [
        sample_header(preset, LENGTH, 60),
        sample_header("EOS", 0, 0),
    ]
    .concat();
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr),
            chunk(b"pbag", &[bag(0), bag(1)].concat()),
            chunk(b"pmod", &[0; 10]),
            chunk(b"pgen", &pgen),
            chunk(b"inst", &inst),
            chunk(b"ibag", &[bag(0), bag(2)].concat()),
            chunk(b"imod", &[0; 10]),
            chunk(b"igen", &igen),
            chunk(b"shdr", &shdr),
        ],
    );

    chunk(b"RIFF", &[b"sfbk".as_slice(), &info, &sdta, &pdta].concat())
}

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn list(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    chunk(b"LIST", &[kind.as_slice(), &chunks.concat()].concat())
}

fn name(text: &str) -> [u8; 20] {
    let mut out = [0; 20];
    for (slot, byte) in out.iter_mut().zip(text.bytes().take(19)) {
        *slot = byte;
    }
    out
}

fn preset_header(preset: &str, bag: u16) -> Vec<u8> {
    let mut out = name(preset).to_vec();
    for value in [0u16, 0, bag] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&[0; 12]);
    out
}

fn sample_header(sample: &str, length: u32, root_key: u8) -> Vec<u8> {
    let mut out = name(sample).to_vec();
    let (loop_start, loop_end) = if length > 0 { (8, length - 8) } else { (0, 0) };
    for value in [0, length, loop_start, loop_end, 44_100] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.push(root_key);
    out.push(0);
    let kind: u16 = if length > 0 { 1 } else { 0 };
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out
}
