//! Usage: cargo run -p audio-engine --example play_midi -- song.mid font.sf2

use std::{env, fs::File, thread, time::Duration};

use audio_engine::{AudioOutput, Player};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let (midi, sf2) = match (args.next(), args.next()) {
        (Some(m), Some(s)) => (m, s),
        _ => return Err("usage: play_midi <file.mid> <file.sf2>".into()),
    };

    let score = io_midi::import_midi(&std::fs::read(midi)?)?;
    let sample_rate = AudioOutput::default_sample_rate()?;
    let player = Player::new(&score, &mut File::open(sf2)?, sample_rate)?;
    let output = AudioOutput::start(player)?;
    println!(
        "\"{}\": {} track(s), {} bpm",
        score.title,
        score.tracks.len(),
        score.tempo_bpm
    );

    output.play();
    thread::sleep(Duration::from_secs(30));
    Ok(())
}
