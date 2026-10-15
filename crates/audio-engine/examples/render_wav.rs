//! Renders a score to a WAV file, with or without the sound packs, to listen to the difference
//! without opening the app. The score is the built-in demo, or a MIDI file.
//!
//! Usage: cargo run -p audio-engine --release --example render_wav -- out.wav [song.mid] [--no-packs]
//! Needs `pnpm fetch:soundfont` to have been run.

use std::{env, fs::File, io::Write, path::Path};

use audio_engine::{pack, Player};

const SAMPLE_RATE: u32 = 44_100;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let use_packs = !args.iter().any(|a| a == "--no-packs");
    let mut paths = args.iter().filter(|a| !a.starts_with("--"));
    let out = paths
        .next()
        .ok_or("usage: render_wav <out.wav> [song.mid] [--no-packs]")?;
    let score = match paths.next() {
        Some(midi) => io_midi::import_midi(&std::fs::read(midi)?)?.score,
        None => score_model::demo_score(),
    };

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/soundfonts");
    let mut player = Player::new(
        &mut File::open(assets.join("GeneralUser-GS.sf2"))?,
        SAMPLE_RATE,
    )?;
    if use_packs {
        let (packs, mut errors) = pack::discover(&[assets.join("packs")]);
        errors.extend(pack::add_packs(&mut player, &packs));
        if let Some(error) = errors.first() {
            return Err(error.clone().into());
        }
    }
    player.load_score(&score);
    player.play();

    let mut samples: Vec<f32> = Vec::new();
    let mut block = vec![0.0f32; 2 * 4096];
    while player.is_playing() {
        player.render(&mut block);
        samples.extend_from_slice(&block);
    }
    write_wav(out, &samples)?;
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "{out}: {:.1} s, peak {peak:.2}, {}",
        samples.len() as f32 / 2.0 / SAMPLE_RATE as f32,
        if use_packs {
            "sound packs"
        } else {
            "General MIDI only"
        }
    );
    Ok(())
}

/// 16-bit stereo PCM.
fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&2u16.to_le_bytes())?; // channels
    file.write_all(&SAMPLE_RATE.to_le_bytes())?;
    file.write_all(&(SAMPLE_RATE * 4).to_le_bytes())?;
    file.write_all(&4u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        file.write_all(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    Ok(())
}
